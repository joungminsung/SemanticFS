# Contributing to SemanticFS

기여를 환영합니다! SemanticFS는 Rust로 작성된 FUSE 기반 시맨틱 파일시스템 오픈소스 프로젝트입니다.

---

## Table of Contents

- [Getting Started](#getting-started)
- [Development Environment](#development-environment)
- [Project Structure](#project-structure)
- [How to Contribute](#how-to-contribute)
- [Development Workflow](#development-workflow)
- [Coding Guidelines](#coding-guidelines)
- [Testing](#testing)
- [Commit Messages](#commit-messages)
- [Pull Request Process](#pull-request-process)
- [Where to Start](#where-to-start)
- [Architecture Guide](#architecture-guide)
- [Troubleshooting](#troubleshooting)

---

## Getting Started

### 1. Fork & Clone

```bash
# Fork on GitHub, then:
git clone https://github.com/<your-username>/SemanticFS.git
cd SemanticFS
```

### 2. Setup (자동)

```bash
./scripts/setup-dev.sh
```

이 스크립트가 자동으로:
- Rust toolchain 설치 (없는 경우)
- `rustfmt`, `clippy`, `cargo-audit` 설치
- macOS: macFUSE + Xcode CLT 설치
- Linux: `libfuse3-dev` 설치
- 빌드 + 테스트 실행

### 3. 확인

```bash
cargo test --workspace     # 39 tests should pass
cargo clippy --workspace   # No warnings
semfs --version            # 0.1.0
```

---

## Development Environment

### 필수

- **Rust** 1.75+ (setup-dev.sh가 자동 설치)
- **C compiler** — SQLite, tree-sitter 빌드에 필요 (macOS: Xcode CLT, Linux: build-essential)

### 선택

- **macFUSE** (macOS) / **libfuse3** (Linux) — FUSE 마운트 기능에 필요
- **Ollama** — 시맨틱 검색 테스트에 필요

### Makefile 사용

```bash
make setup          # 전체 셋업 (./scripts/setup-dev.sh)
make build          # cargo build --workspace
make test           # cargo test --workspace
make lint           # cargo clippy --workspace -- -D warnings
make fmt            # cargo fmt --all
make ci             # fmt-check + lint + test (PR 전 확인용)
make install        # semfs 바이너리 설치
```

---

## Project Structure

```
SemanticFS/
├── crates/
│   ├── semfs-storage/       # 저장 계층
│   │   ├── sqlite.rs        #   SQLite + FTS5 메타데이터/검색
│   │   ├── lance.rs          #   벡터 스토어 (cosine similarity)
│   │   ├── wal.rs            #   Write-Ahead Log
│   │   ├── cache.rs          #   3-Layer 캐시 (L1 쿼리/L2 임베딩/L3 파싱)
│   │   └── types.rs          #   공유 타입 정의
│   │
│   ├── semfs-embed/         # 임베딩 추상화
│   │   ├── traits.rs         #   Embedder trait 정의
│   │   ├── ollama.rs         #   Ollama HTTP API 클라이언트
│   │   ├── onnx.rs           #   ONNX Runtime 추론
│   │   └── noop.rs           #   폴백 (임베딩 없이 키워드만)
│   │
│   ├── semfs-core/          # 핵심 엔진
│   │   ├── query/            #   자연어 파싱 (날짜/확장자 필터 추출)
│   │   ├── retriever/        #   하이브리드 검색 (시맨틱 + 키워드 + RRF)
│   │   ├── indexer/          #   파일 크롤링 + 청킹 + 인덱싱 파이프라인
│   │   │   └── chunker/      #     tree-sitter AST 기반 코드 청킹
│   │   └── vfs/              #   가상 파일시스템 매핑 + Write 핸들러
│   │
│   ├── semfs-watch/         # 파일 감시
│   │   ├── watcher.rs        #   notify 기반 디렉토리 감시
│   │   └── debounce.rs       #   이벤트 디바운싱
│   │
│   ├── semfs-fuse/          # FUSE 추상화
│   │   ├── filesystem.rs     #   fuser::Filesystem 구현
│   │   ├── provider.rs       #   플랫폼별 추상화 trait
│   │   ├── linux.rs / macos.rs / windows.rs
│   │   └── (fuse-mount feature flag — macFUSE/libfuse3 필요)
│   │
│   └── semfs-cli/           # CLI
│       ├── main.rs           #   clap 기반 커맨드 라우팅
│       ├── config.rs         #   TOML 설정 관리
│       └── commands/         #   mount, search, index, diagnose, etc.
│
├── tests/                   # 통합 테스트 + proptest
├── benches/                 # 벤치마크
├── scripts/
│   ├── setup-dev.sh         # 원스텝 개발 환경 셋업
│   └── install.sh           # 사용자용 설치 스크립트
└── docs/specs/              # 설계 문서
```

### Crate 의존성 그래프

```
semfs-cli
  └── semfs-fuse (optional, feature = "fuse")
        └── semfs-core
              ├── semfs-storage
              ├── semfs-embed
              └── semfs-watch
```

**모든 의존성은 단방향입니다.** 하위 crate는 상위를 모릅니다.

### 어떤 crate를 수정해야 하나요?

| 하고 싶은 것 | 수정할 crate |
|-------------|-------------|
| 새 언어 지원 추가 (예: Kotlin) | `semfs-core/indexer/chunker/code.rs` |
| 쿼리 파서에 새 필터 추가 | `semfs-core/query/filter.rs` |
| 새 임베딩 모델 provider 추가 | `semfs-embed/` |
| 검색 알고리즘 개선 | `semfs-core/retriever/` |
| CLI 명령어 추가 | `semfs-cli/commands/` |
| 캐시 전략 수정 | `semfs-storage/cache.rs` |
| FUSE 동작 수정 | `semfs-fuse/filesystem.rs` |
| 새 파일 타입 인덱싱 | `semfs-core/indexer/chunker/` |

---

## How to Contribute

### 기여 유형

1. **Bug Report** — [이슈 템플릿](https://github.com/joungminsung/SemanticFS/issues/new?template=bug_report.yml) 사용
2. **Feature Request** — [이슈 템플릿](https://github.com/joungminsung/SemanticFS/issues/new?template=feature_request.yml) 사용
3. **Code** — 버그 수정, 새 기능, 리팩토링
4. **Documentation** — README, 코드 주석, 아키텍처 문서
5. **Testing** — 새 테스트 케이스, 프로퍼티 테스트
6. **Review** — 다른 PR 리뷰

### 버그 리포트 시 포함할 것

```bash
# 이 출력을 이슈에 붙여주세요
semfs diagnose --json
```

---

## Development Workflow

### 1. 이슈 확인

작업 전 관련 이슈가 있는지 확인하세요. 없으면 먼저 이슈를 생성합니다.

### 2. 브랜치 생성

```bash
git checkout -b feat/add-kotlin-chunker
# or: fix/query-parser-date-range
# or: docs/improve-architecture-guide
```

### 3. 개발

```bash
# 특정 crate만 빌드 (빠름)
cargo build -p semfs-core

# 특정 crate만 테스트
cargo test -p semfs-core

# 전체 빌드
cargo build --workspace
```

### 4. PR 제출 전 확인

```bash
make ci   # fmt-check + clippy + test
```

또는 개별로:

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

---

## Coding Guidelines

### Rust 스타일

- `cargo fmt` 결과를 따릅니다 (`rustfmt.toml` 참조)
- `cargo clippy` 경고 0개를 유지합니다
- `unsafe`는 FUSE/FFI 경계에서만 허용합니다

### 코드 구조

- **Trait 기반 추상화**: 새 기능은 기존 trait을 구현하거나, 필요하면 새 trait을 정의합니다
  ```rust
  // 새 임베딩 provider 추가하기:
  impl Embedder for MyNewEmbedder {
      fn embed_text(&self, text: &str) -> Result<Vec<f32>> { ... }
      fn dimensions(&self) -> usize { ... }
      fn model_name(&self) -> &str { ... }
  }
  ```

- **에러 처리**: 각 crate의 `error.rs`에 정의된 에러 타입 사용. `unwrap()` 금지 (테스트 제외)

- **로깅**: `tracing` 매크로 사용
  ```rust
  use tracing::{debug, info, warn, error};
  debug!(file_id, path = %path.display(), "Processing file");
  ```

### 하지 말 것

- 요청되지 않은 리팩토링
- 불필요한 추상화 레이어 추가
- `#[allow(clippy::...)]` 남발
- 거대한 PR (500줄 이상이면 분리 고려)

---

## Testing

### 테스트 레벨

| 레벨 | 위치 | 실행 |
|------|------|------|
| **Unit** | 각 모듈 내 `#[cfg(test)]` | `cargo test -p semfs-core` |
| **Integration** | `tests/integration_test.rs` | `cargo test --test integration_test` |
| **Property** | `tests/integration_test.rs` (proptest) | `cargo test --test integration_test` |

### 테스트 작성 가이드

```rust
// 좋은 예: 의미 있는 테스트명 + 명확한 assertion
#[test]
fn test_query_parser_extracts_year_filter() {
    let q = parse_query("2024년 React 프로젝트");
    assert_eq!(q.filters.len(), 1);
    assert!(matches!(&q.filters[0], QueryFilter::DateRange { .. }));
}

// 좋은 예: proptest로 불변성 검증
proptest! {
    #[test]
    fn query_parser_never_panics(input in "\\PC{0,500}") {
        let _ = parse_query(&input);  // 어떤 입력이든 패닉하면 안 됨
    }
}
```

### 새 언어 청커 추가 시 필수 테스트

```rust
#[test]
fn test_kotlin_chunking() {
    let content = r#"
class Server(val port: Int) {
    fun start() { println("Starting") }
}
fun main() { Server(8080).start() }
"#;
    let chunker = CodeChunker::new();
    let chunks = chunker.chunk(Path::new("main.kt"), content);
    assert!(!chunks.is_empty());
    assert!(chunks.iter().any(|c| c.chunk_type == ChunkType::Function));
}
```

---

## Commit Messages

[Conventional Commits](https://www.conventionalcommits.org/) 형식을 사용합니다:

```
<type>(<scope>): <description>

[optional body]
```

### Types

| Type | 설명 | 예시 |
|------|------|------|
| `feat` | 새 기능 | `feat(core): add Kotlin language support for chunker` |
| `fix` | 버그 수정 | `fix(storage): handle concurrent WAL writes` |
| `docs` | 문서 | `docs: add architecture diagram to README` |
| `test` | 테스트 | `test(embed): add property tests for Ollama embedder` |
| `refactor` | 리팩토링 | `refactor(retriever): extract RRF into separate module` |
| `perf` | 성능 개선 | `perf(indexer): batch embedding calls for 3x speedup` |
| `ci` | CI/CD | `ci: add Windows build to release workflow` |
| `chore` | 기타 | `chore: update tree-sitter dependencies` |

### Scope

crate 이름을 scope로 사용: `core`, `storage`, `embed`, `watch`, `fuse`, `cli`

---

## Pull Request Process

### 1. PR 생성

- 제목: Conventional Commit 형식 (예: `feat(core): add Kotlin chunker`)
- 본문: [PR 템플릿](.github/PULL_REQUEST_TEMPLATE.md)을 따릅니다

### 2. 체크리스트

- [ ] `make ci` 통과 (fmt + clippy + test)
- [ ] 새 기능이면 테스트 포함
- [ ] 파일 추가/삭제 시 `mod.rs` 업데이트
- [ ] CHANGELOG.md 업데이트
- [ ] 큰 변경이면 이슈 먼저 논의

### 3. 리뷰

- 메인테이너가 리뷰합니다
- 피드백에 대한 토론은 환영합니다
- 승인 후 squash merge합니다

---

## Where to Start

### Good First Issues

`good-first-issue` 라벨이 붙은 이슈는 새 기여자를 위해 준비되었습니다:

- 기여에 필요한 컨텍스트가 이슈에 포함되어 있습니다
- 어떤 파일을 수정해야 하는지 명시되어 있습니다
- 예상 난이도와 소요 시간이 표시되어 있습니다

### 기여하기 좋은 영역

| 영역 | 난이도 | 설명 |
|------|--------|------|
| 새 언어 지원 추가 | Easy | `code.rs`에 tree-sitter grammar + 노드 매핑 추가 |
| 쿼리 파서 필터 확장 | Easy | `filter.rs`에 새 패턴 추가 (예: 파일 크기) |
| CLI 명령어 추가 | Easy | `commands/` 디렉토리에 새 서브커맨드 |
| 검색 품질 개선 | Medium | `retriever/`에서 RRF 파라미터 튜닝 |
| 벤치마크 작성 | Medium | `benches/`에 criterion 벤치마크 |
| LanceDB 통합 | Hard | `lance.rs`를 실제 LanceDB ANN으로 교체 |
| ONNX 토크나이저 | Hard | `onnx.rs`에 `tokenizers` crate 통합 |

---

## Architecture Guide

더 깊은 아키텍처 이해가 필요하면 [Design Specification](docs/specs/2026-03-30-semanticfs-design.md)을 참조하세요.

### 데이터 흐름: 인덱싱

```
crawl_directory()
    → 파일 목록
    → 파일별: SHA-256 해시 비교 (변경된 것만)
    → get_chunker() → tree-sitter AST 파싱 → 계층적 Chunk 생성
    → Embedder.embed_batch() → 벡터 생성
    → SqliteStore.insert_file() + insert_chunk() + index_content()
    → LanceStore.insert() (벡터 저장)
```

### 데이터 흐름: 검색

```
parse_query("2024년 React 프로젝트")
    → ParsedQuery { semantic: "React 프로젝트", filters: [DateRange(2024)] }
    → HybridRetriever.search()
        → SemanticRetriever: query embedding → cosine similarity
        → KeywordRetriever: FTS5 search
        → RRF fusion
        → metadata filter 적용
    → Vec<SearchResult>
```

### 데이터 흐름: Write (mv/cp/rm)

```
FUSE rename() callback
    → WriteHandler.handle_rename()
        → WAL.log_operation(Move { from, to })  // pending
        → WAL.mark_executing()
        → std::fs::rename(from, to)              // 실제 파일 이동
        → WAL.mark_completed()
        → Indexer: 비동기 재인덱싱
```

---

## Troubleshooting

### 빌드 에러

**"fuse.pc not found"**
```bash
# macOS
brew install --cask macfuse
# Linux
sudo apt install fuse3 libfuse3-dev
# 또는 FUSE 없이 빌드:
cargo build -p semfs-cli  # (mount 명령만 비활성화)
```

**tree-sitter C compilation 에러**
```bash
# macOS
xcode-select --install
# Linux
sudo apt install build-essential
```

### 런타임 에러

**"No embedding model found"**
```bash
# Ollama 설치 + 모델 다운로드
ollama pull multilingual-e5-base
# 또는 키워드 검색만 사용 (임베딩 없이도 동작)
```

### 질문이 있으면

- [GitHub Issues](https://github.com/joungminsung/SemanticFS/issues)에서 질문해주세요
- `question` 라벨을 사용합니다

---

## Code of Conduct

이 프로젝트는 [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md)를 따릅니다.

감사합니다!
