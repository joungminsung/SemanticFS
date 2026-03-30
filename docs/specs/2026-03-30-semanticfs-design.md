# SemanticFS - Production Design Specification

> **Date**: 2026-03-30
> **Status**: Approved
> **Based on**: PRD v1.0.0

---

## 1. Architecture Overview

SemanticFS는 FUSE 기반 시맨틱 파일시스템으로, 자연어 경로를 통해 파일에 접근한다.
모놀리식 바이너리 + Cargo workspace 내부 crate 분리 구조를 채택한다.

### 1.1 Design Decisions

| 항목 | 결정 | 근거 |
|------|------|------|
| 임베딩 전략 | Ollama + ONNX 동등, FTS5 폴백 | Progressive enhancement |
| 크로스플랫폼 | Linux + macOS + Windows | FUSE trait 추상화 |
| Write 지원 | Full Write (mv, cp, rm) | 양방향 매핑 |
| 동시성 | 멀티스레드 + crossbeam 채널 | fuser 호환, sync API 활용 |
| 캐싱 | 다층 (L1 쿼리 / L2 임베딩 / L3 파싱) | 정밀 무효화 |
| 다국어 | 다국어 임베딩 모델 단일 사용 | multilingual-e5-base |
| 청킹 | 계층적 (tree-sitter + 부모-자식) | 검색 정확도 최대화 |
| 에러 복구 | WAL 기반 | 원자성 보장 |
| 배포 | 단일 바이너리 + 패키지 매니저 | 설치 용이성 |
| 관측성 | 로깅 + 메트릭 + 진단 CLI | 문제 해결 속도 |
| 보안 | 퍼미션 상속 + ACL + 샌드박스 | 심층 방어 |
| 테스트 | 유닛 + 통합 + 프로퍼티 기반 | 불변성 검증 |
| 확장성 | 내부 trait 추상화 | YAGNI, 나중에 플러그인으로 승격 가능 |

### 1.2 Crate Dependency Graph

```
semfs-cli
  └── semfs-fuse
        └── semfs-core
              ├── semfs-storage
              ├── semfs-embed
              └── semfs-watch
```

모든 의존성은 단방향. 하위 crate는 상위를 모른다.

### 1.3 Thread Model

```
[FUSE Thread Pool]  ──(crossbeam)──▶  [Indexer Thread]
       │                                      │
   read/search                          embed/index
       │                                      │
  semfs-storage (read)              semfs-storage (write)

[Watcher Thread]  ──(crossbeam)──▶  [Indexer Thread]
       │
  FS events → debounce → enqueue
```

Write 조작: FUSE 스레드에서 WAL 기록 → 반환 → Worker 스레드에서 실제 실행.

---

## 2. Crate Specifications

### 2.1 semfs-storage

SQLite(메타데이터 + FTS5) + LanceDB(벡터) + WAL + 다층 캐시.

**Public Traits:**
```rust
trait MetadataStore {
    fn insert_file(&self, meta: FileMeta) -> Result<FileId>;
    fn update_file(&self, id: FileId, meta: FileMeta) -> Result<()>;
    fn delete_file(&self, id: FileId) -> Result<()>;
    fn get_file(&self, id: FileId) -> Result<FileMeta>;
    fn search_fts(&self, query: &str) -> Result<Vec<(FileId, f32)>>;
    fn filter_by(&self, filter: &MetadataFilter) -> Result<Vec<FileId>>;
}

trait VectorStore {
    fn insert(&self, id: FileId, chunks: &[ChunkEmbedding]) -> Result<()>;
    fn search(&self, query_vec: &[f32], top_k: usize) -> Result<Vec<(FileId, f32)>>;
    fn delete(&self, id: FileId) -> Result<()>;
}

trait WriteAheadLog {
    fn log_operation(&self, op: FileOperation) -> Result<WalEntryId>;
    fn mark_completed(&self, id: WalEntryId) -> Result<()>;
    fn recover_pending(&self) -> Result<Vec<FileOperation>>;
}
```

**캐시 계층:**
- L1: `HashMap<QueryHash, Vec<SearchResult>>` — LRU 1,000엔트리, 쿼리 결과
- L2: `DiskCache<FileHash, Vec<f32>>` — 파일 해시 → 임베딩, 디스크 기반
- L3: `HashMap<String, ParsedQuery>` — 자연어 → 파싱 결과 LRU

**무효화 전략:**
- 파일 변경 → L2 해당 파일 갱신 → L1에서 해당 파일 포함 쿼리 무효화
- L3는 TTL 기반 (5분)

**SQLite Schema:**
```sql
CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    extension TEXT,
    size INTEGER NOT NULL,
    hash TEXT NOT NULL,         -- SHA-256
    created_at INTEGER NOT NULL,
    modified_at INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL,
    mime_type TEXT
);

CREATE VIRTUAL TABLE files_fts USING fts5(
    name, path, content,
    content_rowid='id'
);

CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id),
    chunk_index INTEGER NOT NULL,
    parent_chunk_id INTEGER REFERENCES chunks(id),  -- 계층적 청킹
    content TEXT NOT NULL,
    chunk_type TEXT NOT NULL,   -- function, class, section, etc.
    start_line INTEGER,
    end_line INTEGER
);

CREATE TABLE wal_entries (
    id INTEGER PRIMARY KEY,
    operation TEXT NOT NULL,    -- move, copy, delete
    source_path TEXT NOT NULL,
    dest_path TEXT,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, executing, completed, failed
    created_at INTEGER NOT NULL,
    completed_at INTEGER
);

CREATE TABLE acl_rules (
    id INTEGER PRIMARY KEY,
    pattern TEXT NOT NULL,      -- glob pattern
    permission TEXT NOT NULL,   -- read, write, deny
    mount_point TEXT NOT NULL
);
```

### 2.2 semfs-embed

임베딩 모델 추상화. Ollama와 ONNX를 동등하게 지원.

**Public Trait:**
```rust
trait Embedder: Send + Sync {
    fn embed_text(&self, text: &str) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}
```

**구현체:**
- `OllamaEmbedder`: HTTP API (`POST /api/embeddings`). 모델: `multilingual-e5-base` 등
- `OnnxEmbedder`: `ort` crate으로 ONNX 직접 추론. 바이너리에 모델 번들 또는 별도 다운로드
- `NoopEmbedder`: 임베딩 없이 FTS5 폴백 시 사용. 빈 벡터 반환

**모델 선택 우선순위:**
1. 설정 파일에 명시된 provider
2. Ollama 감지 → Ollama 사용
3. ONNX 모델 파일 존재 → ONNX 사용
4. 둘 다 없음 → NoopEmbedder + FTS5 폴백

### 2.3 semfs-core

Query Parser, Hybrid Retriever, Indexer, VFS Mapper.

#### Query Parser
```rust
struct ParsedQuery {
    semantic_query: String,        // 임베딩 검색에 사용
    filters: Vec<MetadataFilter>,  // 날짜, 확장자, 크기
    sort: SortOrder,
    raw_input: String,
}

enum MetadataFilter {
    DateRange { start: DateTime, end: DateTime },
    Extension(Vec<String>),
    Size { min: Option<u64>, max: Option<u64> },
    MimeType(Vec<String>),
}
```

파싱 전략:
- 정규식으로 날짜 패턴 추출 ("2024년", "지난달", "최근 3일")
- 키워드 → 확장자 매핑 ("TypeScript" → `.ts,.tsx`, "Python" → `.py`)
- 나머지 텍스트 → `semantic_query`

#### Hybrid Retriever
```rust
trait Retriever {
    fn search(&self, query: &ParsedQuery, top_k: usize) -> Result<Vec<SearchResult>>;
}

struct HybridRetriever {
    alpha: f32,  // semantic weight (default 0.7)
    vector_store: Arc<dyn VectorStore>,
    metadata_store: Arc<dyn MetadataStore>,
    embedder: Arc<dyn Embedder>,
}
```

검색 흐름:
1. `ParsedQuery`에서 필터 적용 → 후보 파일 집합
2. 시맨틱 검색: query 임베딩 → LanceDB ANN 검색
3. 키워드 검색: FTS5 검색
4. RRF 병합: `score(d) = Σ 1/(k + rank_i(d))`
5. 필터 교집합 → 최종 결과

#### Indexer
```rust
trait Chunker: Send + Sync {
    fn supported_extensions(&self) -> &[&str];
    fn chunk(&self, path: &Path, content: &str) -> Result<Vec<Chunk>>;
}

struct Chunk {
    content: String,
    chunk_type: ChunkType,  // Function, Class, Module, Section, Paragraph
    parent: Option<usize>,  // 부모 청크 인덱스
    start_line: usize,
    end_line: usize,
    metadata: HashMap<String, String>,  // language, name 등
}
```

인덱싱 파이프라인:
1. 크롤러가 소스 디렉토리 순회
2. 파일 해시 비교 → 변경된 파일만 처리
3. 확장자 기반 Chunker 선택
4. 계층적 청킹 (tree-sitter AST)
5. 배치 임베딩 (100개 단위)
6. LanceDB + SQLite 저장

#### VFS Mapper (Write Operations)
```rust
struct VfsMapper {
    wal: Arc<dyn WriteAheadLog>,
    source_root: PathBuf,
}

impl VfsMapper {
    fn handle_rename(&self, from: &Path, to: &Path) -> Result<()>;  // mv
    fn handle_copy(&self, from: &Path, to: &Path) -> Result<()>;    // cp
    fn handle_unlink(&self, path: &Path) -> Result<()>;             // rm (soft delete)
    fn handle_write(&self, path: &Path, data: &[u8]) -> Result<()>; // write
}
```

Write 흐름:
1. WAL에 intent 기록 (pending)
2. 원본 파일에 실제 조작 실행
3. 인덱스 업데이트 (비동기)
4. WAL 완료 마킹

### 2.4 semfs-watch

파일시스템 변경 감지.

```rust
trait FileWatcher: Send {
    fn watch(&mut self, path: &Path) -> Result<()>;
    fn unwatch(&mut self, path: &Path) -> Result<()>;
}

enum FsEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}
```

`notify` crate 사용. 5초 디바운스 → 배치로 Indexer에 전달 (crossbeam channel).

### 2.5 semfs-fuse

FUSE 추상화 + 플랫폼별 구현.

```rust
trait FuseProvider: Send + Sync {
    fn mount(&self, source: &Path, mountpoint: &Path, options: &MountOptions) -> Result<()>;
    fn unmount(&self, mountpoint: &Path) -> Result<()>;
}
```

플랫폼별:
- Linux: `fuser` crate (libfuse3)
- macOS: `fuser` crate (macFUSE)
- Windows: `winfsp-rs` 또는 `dokan-rs`

FUSE 콜백 → semfs-core 호출 매핑:
- `readdir(path)` → QueryParser.parse(path) → Retriever.search() → 가상 엔트리 목록
- `read(path, offset, size)` → 원본 파일에서 직접 읽기
- `write(path, data)` → VfsMapper.handle_write()
- `rename(from, to)` → VfsMapper.handle_rename()
- `unlink(path)` → VfsMapper.handle_unlink()

### 2.6 semfs-cli

```
semfs mount <source> <mountpoint> [--model MODEL] [--read-only]
semfs unmount <mountpoint>
semfs index <source> [--full]
semfs search <query> [--limit N]
semfs status
semfs config set <key> <value>
semfs config get <key>
semfs diagnose [query|index|cache] [--json]
```

---

## 3. Security Model

### 3.1 Permission Inheritance
시맨틱 경로의 파일 접근 시 원본 파일의 Unix 퍼미션 확인.

### 3.2 ACL Layer
```toml
# config.toml
[[acl]]
pattern = "*.env"
permission = "deny"

[[acl]]
pattern = "/secrets/**"
permission = "deny"
```

### 3.3 Sandbox
- Linux: seccomp-bpf로 FUSE 프로세스 시스템콜 제한
- macOS: sandbox-exec 프로파일
- 인덱서가 소스 디렉토리 + `~/.semanticfs/` 외부 접근 차단

---

## 4. Open Source Structure

### 4.1 Repository Layout
```
semanticfs/
├── .github/
│   ├── workflows/
│   │   ├── ci.yml           # PR 체크: lint + test + build
│   │   ├── release.yml      # 태그 → 플랫폼별 바이너리 빌드 + GitHub Release
│   │   └── audit.yml        # cargo-audit 보안 검사
│   ├── ISSUE_TEMPLATE/
│   │   ├── bug_report.yml
│   │   └── feature_request.yml
│   ├── PULL_REQUEST_TEMPLATE.md
│   └── CODEOWNERS
├── crates/                  # 내부 crate들
├── tests/                   # 통합 테스트
├── benches/                 # 벤치마크
├── docs/
│   ├── architecture.md      # 아키텍처 개요 (기여자용)
│   └── specs/               # 설계 문서
├── scripts/
│   ├── install.sh           # curl -sSL 원라이너용
│   └── setup-dev.sh         # 개발 환경 셋업
├── Cargo.toml               # workspace
├── LICENSE-MIT
├── LICENSE-APACHE
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── CHANGELOG.md
├── CLAUDE.md
├── README.md
├── rustfmt.toml
├── clippy.toml
├── deny.toml                # cargo-deny 설정
└── .gitignore
```

### 4.2 Contributor Experience
- **Good first issues**: 라벨링 체계 (`good-first-issue`, `help-wanted`, `bug`, `enhancement`)
- **Crate 단위 기여**: 각 crate가 독립적이라 특정 영역만 이해하고 기여 가능
- **Feature flags**: `onnx`, `ollama`, `clipboard`, `image` 등 optional 기능을 Cargo features로 관리
- **Dual license**: MIT + Apache-2.0 (Rust 생태계 표준)

### 4.3 CI Pipeline
1. `cargo fmt --check`
2. `cargo clippy -- -D warnings`
3. `cargo test --workspace`
4. `cargo test --workspace --features onnx`
5. 플랫폼별 빌드 (Linux x86_64, macOS arm64/x86_64, Windows x86_64)
6. `cargo audit`
7. `cargo deny check`

---

## 5. Data Directory

```
~/.semanticfs/
├── config.toml          # 사용자 설정
├── index.db             # SQLite (메타데이터 + FTS5)
├── vectors.lance/       # LanceDB 벡터 데이터
├── wal/                 # Write-Ahead Log
├── cache/
│   ├── embeddings/      # L2 임베딩 캐시
│   └── queries/         # L1 쿼리 캐시 (메모리 기반, 디스크 백업)
├── models/              # ONNX 모델 파일
└── logs/
    └── semfs.log        # 구조화 로그 (JSON)
```

---

## 6. Performance Targets

| Metric | Target | 측정 방법 |
|--------|--------|-----------|
| 초기 인덱싱 | 1,000 파일/분 | benches/indexing.rs |
| 쿼리 응답 (warm) | < 200ms | benches/query.rs |
| 쿼리 응답 (cold) | < 2s | benches/query.rs |
| 메모리 (10만 파일) | < 500MB | `semfs diagnose` |
| FUSE readdir | < 100ms | benches/fuse.rs |
| 증분 인덱싱 | < 1s (단일 파일) | benches/indexing.rs |
