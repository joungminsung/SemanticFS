<p align="center">
  <h1 align="center">SemanticFS</h1>
  <p align="center">
    <strong>FUSE-based semantic filesystem — access files by meaning, not paths</strong>
  </p>
  <p align="center">
    <a href="#quick-start">Quick Start</a> |
    <a href="#installation">Installation</a> |
    <a href="#how-it-works">How It Works</a> |
    <a href="#contributing">Contributing</a>
  </p>
</p>

---

> **"폴더 정리는 끝났다. 의미로 찾아라."**

SemanticFS는 기존 디렉토리 경로 대신 **자연어**로 파일에 접근할 수 있게 하는 FUSE 기반 파일시스템입니다. 파일은 원본 위치에 그대로 있고, SemanticFS가 그 위에 의미 기반 뷰를 제공합니다.

```bash
cd /mnt/semantic

ls "React 프로젝트"
# users.route.ts    auth.controller.ts    middleware.ts

ls "2024년에 작성한 TypeScript 파일"
# app.tsx    config.ts    utils.ts

cat "최근 수정한 README"/README.md
# ...file contents...

ls "에러 로그가 포함된 파일"
# server.log    error-handler.ts    crash-report.txt
```

같은 파일이 쿼리에 따라 다른 가상 경로에 나타납니다.

## Why SemanticFS?

**1960년대부터 변하지 않은 계층적 디렉토리 구조의 한계:**

- `report.pdf`는 `/work/2024/`에 넣어야 하나, `/projects/clientA/`에 넣어야 하나?
- "지난달에 작업한 API 관련 파일"을 찾으려면 폴더를 하나하나 뒤져야 합니다
- Spotlight/Everything은 키워드 매칭일 뿐, **의미 기반 검색**이 불가능합니다

**SemanticFS는 이 문제를 파일시스템 레벨에서 해결합니다:**

| 기존 | SemanticFS |
|------|-----------|
| `/home/user/projects/2024/react/my-app/src/App.tsx` | `ls "React 메인 컴포넌트"` |
| Spotlight 키워드 검색 | 시맨틱 + 키워드 하이브리드 검색 |
| 별도 앱 필요 (Obsidian, Notion) | `ls`, `cat`, `vim`, VS Code 등 기존 도구 그대로 |
| 온라인 API 호출 | 완전 로컬, 오프라인 동작 |

## Features

- **자연어 경로** — `ls "에러 로그가 포함된 파일"` 같은 의미 기반 탐색
- **하이브리드 검색** — 시맨틱 임베딩 + FTS5 키워드 검색을 [RRF](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf)로 병합
- **계층적 청킹** — tree-sitter AST 파싱으로 코드는 함수/클래스 단위, 문서는 섹션 단위로 인덱싱
- **한국어 + 영어 혼합 쿼리** — 다국어 임베딩 모델로 네이티브 지원
- **Full Write 지원** — `mv`, `cp`, `rm`이 WAL(Write-Ahead Log)로 보호된 실제 파일 조작으로 매핑
- **Progressive Enhancement** — Ollama 있으면 시맨틱, ONNX도 가능, 둘 다 없으면 키워드 폴백
- **크로스플랫폼** — Linux, macOS, Windows 지원 (FUSE 추상화 레이어)
- **제로 설정** — `semfs mount ~/Documents /mnt/semantic` 한 줄이면 동작
- **3-Layer 캐시** — 쿼리 결과 / 임베딩 / 파싱된 쿼리 각각 독립 캐시 + 정밀 무효화

## Quick Start

**Rust, FUSE, 빌드 도구 — 전부 자동으로 설치됩니다:**

```bash
git clone https://github.com/joungminsung/SemanticFS.git
cd SemanticFS
./scripts/setup-dev.sh   # Rust + FUSE + 빌드 + 테스트 원스텝
```

설치 후 바로 사용:

```bash
# 시스템에 설치
cargo install --path crates/semfs-cli

# 파일 인덱싱
semfs index ~/Documents

# 자연어로 검색 (마운트 없이도 동작)
semfs search "React 프로젝트"
semfs search "2024년에 작성한 Python 코드"
semfs search "에러 처리 관련 함수"

# 상태 확인
semfs status

# 진단
semfs diagnose
```

### FUSE 마운트 (선택 — macFUSE/libfuse3 설치 필요)

```bash
# macOS: brew install --cask macfuse
# Linux: sudo apt install fuse3 libfuse3-dev

# FUSE 지원 빌드
cargo build --workspace --features semfs-cli/fuse

mkdir -p /tmp/semantic
semfs mount ~/Documents /tmp/semantic

# 자연어로 파일 탐색
cd /tmp/semantic
ls "TypeScript API 라우터"
cat "최근 수정한 README"/README.md
```

## Installation

### Option 1: From Source (추천)

```bash
git clone https://github.com/joungminsung/SemanticFS.git
cd SemanticFS
./scripts/setup-dev.sh   # Rust, FUSE, 빌드 도구 자동 설치
cargo install --path crates/semfs-cli
```

### Option 2: One-liner

```bash
curl -sSL https://raw.githubusercontent.com/joungminsung/SemanticFS/main/scripts/install.sh | sh
```

pre-built 바이너리를 먼저 시도하고, 없으면 자동으로 Rust 설치 + 소스 빌드합니다.

### Option 3: Cargo

```bash
cargo install semanticfs
```

### `setup-dev.sh`가 하는 일

| Step | What | Auto? |
|------|------|:---:|
| Rust toolchain | `rustup` + stable (1.75+) | Yes |
| Dev tools | `rustfmt`, `clippy`, `cargo-audit` | Yes |
| FUSE (macOS) | macFUSE via Homebrew | Yes (커널 확장 수동 승인 필요) |
| FUSE (Linux) | `libfuse3-dev` via apt/dnf/pacman | Yes |
| Xcode CLT (macOS) | C compiler (SQLite, tree-sitter 빌드용) | Yes |
| Build + Test | `cargo build && cargo test` | Yes |

### Embedding Model (선택)

임베딩 모델 없이도 **FTS5 키워드 검색**으로 동작합니다. 시맨틱 검색을 원하면:

```bash
# Option A: Ollama (추천)
brew install ollama        # or: curl -fsSL https://ollama.ai/install.sh | sh
ollama serve &
ollama pull multilingual-e5-base

# Option B: ONNX (서버 없이, 빌드 시 --features semfs-embed/onnx)
mkdir -p ~/.semanticfs/models
# all-MiniLM-L6-v2.onnx 다운로드 후 위 경로에 배치
```

## How It Works

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  User: ls "React 프로젝트"                                   │
│         ↓                                                    │
│  ┌─────────────┐   ┌──────────────┐   ┌──────────────────┐  │
│  │  semfs-cli   │──▶│  semfs-fuse   │──▶│   semfs-core     │  │
│  │  (clap CLI)  │   │ (FUSE mount) │   │                  │  │
│  └─────────────┘   └──────────────┘   │ ┌──────────────┐ │  │
│                                        │ │ Query Parser │ │  │
│                                        │ │ "React 프로젝트"│ │  │
│                                        │ │  → semantic   │ │  │
│                                        │ │  + filters    │ │  │
│                                        │ └──────┬───────┘ │  │
│                                        │        ↓         │  │
│                                        │ ┌──────────────┐ │  │
│  ┌──────────────┐                      │ │   Hybrid     │ │  │
│  │ semfs-embed   │◀─────────────────────│ │  Retriever   │ │  │
│  │ Ollama/ONNX  │  embed query          │ │ (RRF fusion) │ │  │
│  └──────────────┘                      │ └──────┬───────┘ │  │
│                                        │        ↓         │  │
│                                        │  ┌─────────────┐ │  │
│                                        │  │ semfs-storage│ │  │
│                                        │  │ SQLite+FTS5  │ │  │
│                                        │  │ VectorStore  │ │  │
│                                        │  │ WAL + Cache  │ │  │
│                                        │  └─────────────┘ │  │
│                                        └──────────────────┘  │
│                                                 ↓             │
│  Result: users.route.ts  auth.controller.ts  middleware.ts   │
└─────────────────────────────────────────────────────────────┘
```

### Crate 구조

```
semanticfs/
├── crates/
│   ├── semfs-storage/   # SQLite + FTS5, Vector Store, WAL, 3-Layer Cache
│   ├── semfs-embed/     # Embedder trait + Ollama / ONNX / Noop 구현
│   ├── semfs-watch/     # notify 기반 파일 감시 + debounce
│   ├── semfs-core/      # Query Parser, Hybrid Retriever (RRF), Indexer, VFS
│   ├── semfs-fuse/      # FUSE 추상화 (Linux/macOS/Windows)
│   └── semfs-cli/       # CLI (clap) — mount, search, index, diagnose 등
├── tests/               # 통합 테스트 + proptest
└── benches/             # 벤치마크
```

각 crate는 독립적으로 이해하고 기여할 수 있습니다. 전체 코드베이스를 알 필요 없습니다.

### 검색 파이프라인

```
"2024년에 작성한 React 프로젝트 중 TypeScript 파일"
                    ↓
            ┌───────────────┐
            │  Query Parser  │
            └───────┬───────┘
                    ↓
  semantic_query: "React 프로젝트"
  filters: [DateRange(2024), Extension(.ts, .tsx)]
                    ↓
     ┌──────────────┴──────────────┐
     ↓                             ↓
┌─────────┐                ┌─────────────┐
│ Semantic │  (embedding)  │   Keyword    │  (FTS5)
│ Search   │                │   Search    │
└────┬────┘                └──────┬──────┘
     └──────────┬─────────────────┘
                ↓
        ┌──────────────┐
        │  RRF Fusion   │  score(d) = Σ 1/(k + rank_i(d))
        └──────┬───────┘
               ↓
        ┌──────────────┐
        │ Filter Apply  │  DateRange + Extension
        └──────┬───────┘
               ↓
         Final Results
```

### Key Design Decisions

| 항목 | 결정 | 근거 |
|------|------|------|
| 임베딩 | Ollama + ONNX 동등 지원 | Progressive enhancement |
| 벡터 검색 | In-memory cosine similarity (LanceDB optional) | 의존성 최소화 |
| 텍스트 검색 | SQLite FTS5 | Zero config, 검증된 기술 |
| 코드 청킹 | tree-sitter AST | 함수/클래스 계층 보존 |
| Write 보호 | WAL (Write-Ahead Log) | 크래시 시 원자성 보장 |
| 동시성 | 멀티스레드 + crossbeam channels | fuser 호환, 명확한 스레드 경계 |
| 캐싱 | L1 쿼리 / L2 임베딩 / L3 파싱 | 정밀 무효화, 레이어별 독립 관리 |

## CLI Reference

```bash
# 마운트/언마운트 (FUSE feature 필요)
semfs mount <source_dir> <mount_point> [--model MODEL] [--read-only]
semfs unmount <mount_point>

# 인덱싱
semfs index <directory>              # 증분 인덱싱
semfs reindex [directory]            # 전체 재인덱싱

# 검색
semfs search <query> [--limit N]     # 자연어 검색

# 설정
semfs config set <key> <value>       # 설정 변경
semfs config get <key>               # 설정 조회

# 진단
semfs status                         # 인덱스 상태
semfs diagnose [query|index|cache]   # 문제 진단
semfs diagnose --json                # JSON 출력 (버그 리포트용)
```

## Configuration

```toml
# ~/.semanticfs/config.toml

[source]
paths = ["~/Documents", "~/Projects"]
ignore = ["node_modules", ".git", "dist", "__pycache__", "*.lock", "target"]
max_file_size = "50MB"

[embedding]
provider = "auto"                # "auto" | "ollama" | "onnx"
model = "multilingual-e5-base"   # 다국어 모델 (한국어+영어)
batch_size = 100
dimensions = 768

[search]
alpha = 0.7                      # 시맨틱 가중치 (0.0=키워드만, 1.0=시맨틱만)
max_results = 100
cache_size = 1000                # L1 쿼리 캐시 엔트리 수

[index]
watch = true                     # 파일 변경 시 자동 재인덱싱
interval = "5s"                  # 디바운스 간격
```

## Performance Targets

| Metric | Target |
|--------|--------|
| Initial indexing | 1,000 files/min |
| Query response (warm cache) | < 200ms |
| Query response (cold) | < 2s |
| Memory (100K files) | < 500MB |
| Incremental reindex (single file) | < 1s |

## Supported File Types

| Category | Extensions | Chunking |
|----------|-----------|----------|
| Source Code | `.rs`, `.py`, `.js`, `.ts`, `.go`, `.java` | tree-sitter AST (함수/클래스/모듈 계층) |
| Text/Docs | `.md`, `.txt`, `.rst` | 섹션/문단 기반 |
| Config/Data | `.json`, `.yaml`, `.toml`, `.csv` | 구조 파싱 |
| Others | `.html`, `.css`, `.sql`, `.sh`, etc. | 텍스트 폴백 |

## Roadmap

- [x] Core: Query Parser + Hybrid Retriever + RRF
- [x] Storage: SQLite + FTS5 + WAL + 3-Layer Cache
- [x] Embedding: Ollama + ONNX + Noop fallback
- [x] Indexer: tree-sitter AST + 계층적 청킹
- [x] CLI: mount, search, index, status, diagnose, config
- [x] FUSE: Linux + macOS + Windows 추상화
- [x] Write: mv/cp/rm with WAL protection
- [ ] LanceDB ANN integration (현재 in-memory cosine)
- [ ] Image embedding (CLIP)
- [ ] PDF/DOCX text extraction
- [ ] Audio/Video (Whisper + CLIP)
- [ ] VS Code extension

## Contributing

기여를 환영합니다! [CONTRIBUTING.md](CONTRIBUTING.md)에서 상세한 가이드를 확인하세요.

```bash
git clone https://github.com/joungminsung/SemanticFS.git
cd SemanticFS
./scripts/setup-dev.sh
cargo test --workspace
```

`good-first-issue` 라벨이 붙은 이슈부터 시작하면 좋습니다.

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE).

## Acknowledgments

- [fuser](https://github.com/cberner/fuser) — Rust FUSE implementation
- [tree-sitter](https://tree-sitter.github.io/) — AST parsing
- [Ollama](https://ollama.ai/) — Local LLM/embedding serving
- [SQLite FTS5](https://www.sqlite.org/fts5.html) — Full-text search
- Gifford et al., "Semantic File Systems" (1991, MIT) — Original concept
