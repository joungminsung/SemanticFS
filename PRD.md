# SemanticFS — Product Requirements Document

> **Version**: 1.0.0
> **Author**: 정민성 (Minseong Jung)
> **Date**: 2026-03-30
> **Status**: Draft

---

## 1. Executive Summary

SemanticFS는 FUSE 기반의 시맨틱 파일시스템으로, 기존의 디렉토리 경로(path) 대신 자연어 의미(semantic meaning)로 파일에 접근할 수 있게 하는 오픈소스 프로젝트다.

`cd "/2024년에 작성한 React 프로젝트"` 같은 자연어 경로로 파일을 탐색하고, 폴더 구조가 쿼리에 따라 동적으로 생성된다. 모든 처리는 로컬에서 수행되며, 외부 API 호출 없이 완전한 오프라인 동작을 보장한다.

### 1.1 One-liner

> **"폴더 정리는 끝났다. 의미로 찾아라."**

### 1.2 Key Metrics (Launch 기준)

| Metric | Target |
|--------|--------|
| 초기 인덱싱 속도 | 1,000 파일 / 분 |
| 쿼리 응답 시간 | < 200ms (warm cache) |
| 메모리 사용량 | < 500MB (10만 파일 기준) |
| GitHub Stars (3개월) | 1,000+ |
| 지원 파일 포맷 | 텍스트, 마크다운, 코드, 이미지 (Phase 1) |

---

## 2. Problem Statement

### 2.1 현재 파일시스템의 근본적 한계

1960년대부터 변하지 않은 계층적 디렉토리 구조는 다음과 같은 문제를 안고 있다:

- **분류의 모호성**: `report.pdf`는 `/work/2024/`에 넣어야 하나, `/projects/clientA/`에 넣어야 하나? 하나의 파일은 하나의 폴더에만 존재할 수 있다.
- **정리 비용**: 개발자의 시간 중 상당 부분이 "파일 어디 뒀지?"에 소비된다. 폴더 구조를 유지하는 것 자체가 인지적 부담이다.
- **검색의 한계**: 기존 검색(Spotlight, Everything)은 파일명/내용의 키워드 매칭일 뿐, "지난달에 작업한 것 중 API 관련 파일"같은 의미 기반 탐색이 불가능하다.
- **컨텍스트 단절**: 파일의 내용, 생성 시점, 관련 파일 간의 의미적 연관성이 디렉토리 구조에 반영되지 않는다.

### 2.2 기존 솔루션의 한계

| 솔루션 | 한계 |
|--------|------|
| Spotlight / Everything | 키워드 매칭만 지원, 의미 검색 불가 |
| Obsidian / Notion | 별도 앱 생태계, 기존 파일과 단절 |
| TagSpaces | 태그 수동 관리 필요, 자동화 없음 |
| macOS Smart Folders | 메타데이터 기반, 내용 기반 의미 검색 불가 |

### 2.3 Target Users

- **Primary**: 파일이 수천~수만 개인 개발자, 연구자, 크리에이터
- **Secondary**: CLI 환경에서 작업하는 시스템 엔지니어, DevOps
- **Tertiary**: 사진/문서 관리가 필요한 일반 사용자

---

## 3. Product Vision

### 3.1 Core Concept

```
전통적 파일시스템:    /home/user/projects/2024/react/my-app/src/App.tsx
SemanticFS:         /semfs/"React 메인 컴포넌트"/App.tsx
                    /semfs/"2024년 프로젝트"/my-app/
                    /semfs/"TypeScript로 작성된 UI 코드"/App.tsx
```

같은 파일이 쿼리에 따라 다른 "가상 경로"에 나타난다. 실제 파일은 원본 위치에 그대로 있고, SemanticFS는 그 위에 의미 기반 뷰(view)를 제공한다.

### 3.2 Design Principles

1. **Zero Configuration**: 설치 후 `semfs mount ~/Documents /mnt/semantic` 한 줄이면 동작
2. **Non-destructive**: 원본 파일을 절대 수정/이동/삭제하지 않음. Read-only 뷰
3. **Offline-first**: 모든 임베딩/검색이 로컬에서 수행. 네트워크 불필요
4. **Tool-agnostic**: vim, cat, ls, VS Code, Finder — 기존 모든 도구와 호환
5. **Progressive Enhancement**: 기본은 키워드, 임베딩 모델 있으면 시맨틱으로 자동 업그레이드

---

## 4. Technical Architecture

### 4.1 System Overview

```
┌─────────────────────────────────────────────────────────┐
│                    User Space                            │
│                                                          │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ Terminal  │  │  File Manager │  │  VS Code / IDE    │  │
│  │ (ls, cd) │  │  (Finder, etc)│  │                   │  │
│  └────┬─────┘  └──────┬───────┘  └─────────┬─────────┘  │
│       │               │                     │            │
│       └───────────────┼─────────────────────┘            │
│                       │                                  │
│              ┌────────▼────────┐                         │
│              │   FUSE Mount    │                         │
│              │  /mnt/semantic  │                         │
│              └────────┬────────┘                         │
│                       │                                  │
│              ┌────────▼────────┐                         │
│              │  SemanticFS     │                         │
│              │  Core Engine    │                         │
│              │                 │                         │
│              │ ┌─────────────┐ │                         │
│              │ │ Query Parser│ │                         │
│              │ └──────┬──────┘ │                         │
│              │        │        │                         │
│              │ ┌──────▼──────┐ │  ┌──────────────────┐  │
│              │ │  Retriever  │◄├──┤  Embedding Model │  │
│              │ │ (Hybrid)    │ │  │  (Ollama/ONNX)   │  │
│              │ └──────┬──────┘ │  └──────────────────┘  │
│              │        │        │                         │
│              │ ┌──────▼──────┐ │                         │
│              │ │  VectorDB   │ │                         │
│              │ │  (LanceDB)  │ │                         │
│              │ └──────┬──────┘ │                         │
│              │        │        │                         │
│              │ ┌──────▼──────┐ │                         │
│              │ │ File Index  │ │                         │
│              │ │  (SQLite)   │ │                         │
│              │ └─────────────┘ │                         │
│              └─────────────────┘                         │
│                       │                                  │
│              ┌────────▼────────┐                         │
│              │  Source Files   │                         │
│              │ ~/Documents     │                         │
│              │ (원본, 수정 없음)│                         │
│              └─────────────────┘                         │
└─────────────────────────────────────────────────────────┘
```

### 4.2 Core Components

#### 4.2.1 FUSE Layer

| 항목 | 설계 |
|------|------|
| 언어 | Rust (`fuser` crate) |
| 마운트 방식 | Read-only mount |
| 경로 해석 | 자연어 경로 → 쿼리 파싱 → 검색 → 가상 디렉토리 반환 |
| 캐싱 | LRU 캐시 (최근 쿼리 결과 1,000개) |

**FUSE 동작 흐름:**

```
사용자: cd "/React 프로젝트"
  → FUSE readdir() 호출
  → "React 프로젝트" 문자열 추출
  → Query Parser로 전달
  → Hybrid Retriever 실행
  → 매칭 파일 목록을 가상 디렉토리 엔트리로 반환
  → 사용자에게 ls 결과 표시
```

#### 4.2.2 Query Parser

자연어 경로에서 구조화된 쿼리를 추출한다.

```
Input:  "/2024년에 작성한 React 프로젝트 중 TypeScript 파일"
Output: {
  semantic_query: "React 프로젝트 TypeScript",
  filters: {
    date_range: { start: "2024-01-01", end: "2024-12-31" },
    extension: [".ts", ".tsx"]
  },
  sort: "relevance"
}
```

**파싱 전략:**

- 날짜 표현 → `date_range` 필터 (정규식 + 패턴 매칭)
- 파일 확장자 키워드 → `extension` 필터 ("TypeScript" → `.ts`, `.tsx`)
- 나머지 → `semantic_query`로 임베딩 검색

#### 4.2.3 Hybrid Retriever

OpenDocuments에서 검증된 하이브리드 검색 전략 적용.

```
Score = α × semantic_score + (1 - α) × keyword_score
```

| 검색 방식 | 엔진 | 용도 |
|-----------|------|------|
| Semantic Search | LanceDB (벡터) | 의미적 유사성 |
| Keyword Search | SQLite FTS5 | 정확한 키워드 매칭 |
| Metadata Filter | SQLite | 날짜, 확장자, 크기 등 |

**RRF (Reciprocal Rank Fusion)** 으로 두 결과를 병합:

```
RRF_score(d) = Σ 1 / (k + rank_i(d))
```

#### 4.2.4 Indexer

파일시스템 변경을 감지하고 임베딩 인덱스를 업데이트한다.

| 항목 | 설계 |
|------|------|
| 파일 감시 | `inotify` (Linux) / `FSEvents` (macOS) |
| 청킹 전략 | 코드: AST 기반, 문서: 섹션 기반, 이미지: 파일 단위 |
| 임베딩 모델 | `nomic-embed-text` (Ollama) 또는 `all-MiniLM-L6-v2` (ONNX) |
| 이미지 임베딩 | `clip-vit-base-patch32` (ONNX) |
| 증분 인덱싱 | 파일 해시(SHA-256) 비교, 변경된 파일만 재인덱싱 |
| 배치 처리 | 100파일 단위 배치 임베딩 |

#### 4.2.5 Storage Layer

```
~/.semanticfs/
├── config.toml          # 설정 파일
├── index.db             # SQLite (메타데이터 + FTS5)
├── vectors.lance/       # LanceDB (벡터 임베딩)
├── cache/               # LRU 쿼리 캐시
└── logs/                # 동작 로그
```

### 4.3 Tech Stack

| Layer | Technology | 선택 근거 |
|-------|-----------|-----------|
| FUSE Interface | Rust + `fuser` | 성능, 안전성, 크로스플랫폼 |
| Vector DB | LanceDB | 서버리스, 파일 기반, 빠른 ANN |
| Metadata DB | SQLite + FTS5 | 제로 설정, 키워드 검색 내장 |
| Embedding (Text) | Ollama / ONNX Runtime | 로컬 실행, 모델 교체 용이 |
| Embedding (Image) | CLIP via ONNX | 이미지 의미 검색 |
| File Watcher | notify (Rust crate) | 크로스플랫폼 inotify/FSEvents |
| CLI | `clap` (Rust) | 표준적인 CLI 프레임워크 |
| Config | TOML | Rust 생태계 표준 |

### 4.4 Supported File Types

#### Phase 1 (MVP)

| 파일 유형 | 확장자 | 임베딩 방식 |
|-----------|--------|------------|
| Plain Text | `.txt`, `.md`, `.rst` | 텍스트 청킹 → 임베딩 |
| Source Code | `.ts`, `.tsx`, `.py`, `.rs`, `.go`, `.java`, etc. | AST 기반 청킹 → 임베딩 |
| Config / Data | `.json`, `.yaml`, `.toml`, `.csv` | 구조 파싱 → 임베딩 |

#### Phase 2

| 파일 유형 | 확장자 | 임베딩 방식 |
|-----------|--------|------------|
| Images | `.png`, `.jpg`, `.webp` | CLIP 임베딩 |
| PDF | `.pdf` | 텍스트 추출 → 임베딩 |
| Office | `.docx`, `.pptx` | 텍스트 추출 → 임베딩 |

#### Phase 3

| 파일 유형 | 확장자 | 임베딩 방식 |
|-----------|--------|------------|
| Audio | `.mp3`, `.wav` | Whisper 로컬 → 텍스트 → 임베딩 |
| Video | `.mp4`, `.mov` | 프레임 추출 → CLIP + Whisper |

---

## 5. User Experience

### 5.1 Installation

```bash
# Option 1: Cargo (Rust)
cargo install semanticfs

# Option 2: Homebrew
brew install semanticfs

# Option 3: npm wrapper (cross-platform)
npx semanticfs init
```

### 5.2 Quick Start

```bash
# 1. 소스 디렉토리 인덱싱 + 마운트 (한 줄)
semfs mount ~/Documents /mnt/semantic

# 2. 바로 사용
cd /mnt/semantic
ls "React 프로젝트"
cat "최근 수정한 README"/README.md
ls "에러 로그가 포함된 파일"
```

### 5.3 CLI Commands

```bash
# 마운트
semfs mount <source_dir> <mount_point> [--model nomic-embed-text]

# 인덱스만 빌드 (마운트 없이)
semfs index <source_dir>

# 인덱스 상태 확인
semfs status

# 재인덱싱 (변경 파일만)
semfs reindex

# 전체 재인덱싱
semfs reindex --full

# 설정
semfs config set model all-MiniLM-L6-v2
semfs config set ignore "node_modules,.git,dist"

# 검색 (마운트 없이 CLI로)
semfs search "2024년 React 프로젝트"

# 언마운트
semfs unmount /mnt/semantic
```

### 5.4 Configuration

```toml
# ~/.semanticfs/config.toml

[source]
paths = ["~/Documents", "~/Projects"]
ignore = ["node_modules", ".git", "dist", "__pycache__", "*.lock"]
max_file_size = "50MB"

[embedding]
provider = "ollama"              # "ollama" | "onnx"
model = "nomic-embed-text"       # text embedding model
image_model = "clip-vit-base"    # image embedding model (Phase 2)
batch_size = 100
dimensions = 768

[search]
alpha = 0.7                      # semantic vs keyword weight
max_results = 100                # max files per query
cache_size = 1000                # LRU cache entries

[index]
watch = true                     # auto-reindex on file change
interval = "5s"                  # file watcher debounce
```

### 5.5 Usage Scenarios

#### Scenario 1: 개발자 — 프로젝트 탐색

```bash
$ cd /mnt/semantic
$ ls "TypeScript API 라우터"
users.route.ts    auth.controller.ts    middleware.ts

$ ls "테스트 파일 중 실패할 것 같은"
payment.test.ts    edge-case.spec.ts
```

#### Scenario 2: 연구자 — 논문/자료 탐색

```bash
$ ls "transformer attention mechanism 관련 논문"
attention-is-all-you-need.pdf    flash-attention.pdf    mqa-paper.pdf

$ ls "2023년 이후 LLM 벤치마크 데이터"
mmlu_results.csv    hellaswag_scores.json
```

#### Scenario 3: 크리에이터 — 사진/영상 탐색 (Phase 2)

```bash
$ ls "해질녘 바다 사진"
IMG_4521.jpg    sunset_jeju.png    beach_2024.jpg

$ ls "사람이 없는 풍경 사진"
mountain_01.jpg    forest_path.png
```

---

## 6. Development Roadmap

### Phase 1: MVP (4주)

> **Goal**: 텍스트 파일 기반 시맨틱 마운트 동작

| Week | Deliverable |
|------|------------|
| W1 | Rust 프로젝트 셋업, FUSE read-only 마운트 기본 구조, SQLite 스키마 |
| W2 | 파일 크롤러 + 텍스트 청킹 + Ollama 임베딩 연동, LanceDB 저장 |
| W3 | Query Parser (날짜/확장자 필터 + 시맨틱 쿼리 분리), Hybrid Retriever |
| W4 | CLI 완성, 자연어 경로 → 가상 디렉토리 매핑, README + 데모 GIF |

**MVP 완료 기준:**
- `semfs mount ~/Documents /mnt/semantic` 으로 마운트
- `ls "/React 프로젝트"` 하면 관련 파일 목록 표시
- `cat` 으로 파일 내용 읽기 가능
- 인덱싱 속도 1,000 파일/분 달성

### Phase 2: Image + Watcher (3주)

| Week | Deliverable |
|------|------------|
| W5 | CLIP 모델 통합, 이미지 임베딩 파이프라인 |
| W6 | `inotify`/`FSEvents` 기반 실시간 파일 감시, 증분 인덱싱 |
| W7 | PDF/DOCX 텍스트 추출, 검색 품질 튜닝 (alpha 최적화) |

### Phase 3: Polish + Community (3주)

| Week | Deliverable |
|------|------------|
| W8 | Homebrew formula, AUR 패키지, 설치 자동화 |
| W9 | VS Code 확장 (semantic 경로 자동완성), 벤치마크 스위트 |
| W10 | HN/Reddit 론칭, 기여 가이드, 아키텍처 문서 |

### Phase 4: Advanced (이후)

- Audio/Video 지원 (Whisper + CLIP)
- 다국어 쿼리 (한국어/영어/일본어 혼합)
- Write 지원 (`mv`, `cp` → 실제 파일 이동)
- 네트워크 파일시스템 (NFS/SMB 위에 시맨틱 레이어)
- WASM 빌드 (브라우저에서 데모)

---

## 7. Technical Challenges & Mitigations

### 7.1 자연어 경로의 모호성

**문제**: "바다 사진"이 여행 사진인지, 바탕화면인지, 그림인지 모호하다.

**해결**:
- Top-K 결과를 모두 보여주고 사용자가 선택하게 함
- 쿼리 refinement: `ls "바다 사진"` → 결과 중 `cd "바다 사진/여행"` 으로 좁히기
- 관련도 threshold 이하는 자동 제외 (configurable)

### 7.2 대량 파일 인덱싱 성능

**문제**: 10만 파일 초기 인덱싱에 시간이 오래 걸린다.

**해결**:
- 배치 임베딩 (100파일 단위)
- 증분 인덱싱 (SHA-256 해시 비교)
- 백그라운드 인덱싱 (마운트 먼저, 인덱싱은 비동기)
- ONNX Runtime 사용 시 CPU에서도 충분한 속도

### 7.3 FUSE 크로스플랫폼

**문제**: macOS는 macFUSE, Windows는 WinFSP 별도 설치 필요.

**해결**:
- Phase 1은 Linux 집중 (FUSE 네이티브)
- macOS: macFUSE 의존성을 설치 스크립트에 포함
- Windows: Phase 3에서 WinFSP 또는 Dokan 지원
- 대안: FUSE 없이 CLI-only 모드 제공 (`semfs search`)

### 7.4 FUSE Read 성능

**문제**: 매번 readdir()에서 임베딩 검색하면 느리다.

**해결**:
- LRU 캐시 (최근 1,000 쿼리 결과 캐싱)
- 캐시 TTL: 파일 변경 감지 시 무효화
- ANN (Approximate Nearest Neighbor)으로 O(log n) 검색

### 7.5 임베딩 모델 없이도 동작

**문제**: Ollama 설치가 부담인 사용자도 있다.

**해결**:
- Fallback 모드: SQLite FTS5 키워드 검색만으로도 동작
- ONNX 내장 모델 옵션: 별도 서버 없이 바이너리에 모델 포함
- Progressive enhancement: 모델 있으면 시맨틱, 없으면 키워드

---

## 8. Competitive Landscape

| 프로젝트 | 접근 방식 | SemanticFS 차별점 |
|----------|-----------|------------------|
| Spotlight / Everything | 키워드 검색 | 의미 기반 검색 + 파일시스템 통합 |
| Obsidian | 자체 앱 생태계 | OS 레벨 통합, 모든 도구 호환 |
| TagSpaces | 수동 태깅 | 자동 임베딩, 제로 설정 |
| Semantic Desktop (학술) | 온톨로지 기반 | 딥러닝 임베딩, 실용적 구현 |
| macOS Smart Folders | 메타데이터 쿼리 | 내용 기반 시맨틱 쿼리 |

### 8.1 Unfair Advantage

1. **파일시스템 레벨**: 앱이 아닌 OS 인프라로 동작 → 모든 도구 호환
2. **완전 로컬**: 프라이버시 내러티브 + 오프라인 동작
3. **Zero Config**: 마운트 한 줄이면 끝
4. **오픈소스**: Rust + LanceDB + Ollama, 모두 검증된 스택

---

## 9. Success Criteria

### 9.1 Launch (Phase 1 완료 시점)

- [ ] `semfs mount` + `ls` + `cat` 정상 동작
- [ ] 1,000파일 인덱싱 < 60초
- [ ] 쿼리 응답 < 200ms
- [ ] 데모 GIF가 포함된 README
- [ ] GitHub 공개 + HN Show HN 포스트

### 9.2 Traction (Phase 3 완료 시점)

- [ ] GitHub Stars 1,000+
- [ ] Homebrew / AUR 패키지 등록
- [ ] 외부 기여자 5명+
- [ ] 벤치마크 결과 공개
- [ ] 기술 블로그 포스트 3개+

### 9.3 Long-term

- [ ] VS Code / JetBrains 확장
- [ ] 크로스플랫폼 (Linux + macOS + Windows)
- [ ] 멀티미디어 지원 (이미지, 오디오, 비디오)
- [ ] 커뮤니티 주도 개발 체제 전환

---

## 10. Open Questions

| # | Question | Impact | Status |
|---|----------|--------|--------|
| 1 | Rust vs Go? Rust가 FUSE 성능은 좋지만 개발 속도는 Go가 빠름 | 아키텍처 전체 | **Decision: Rust** (성능 + 학습 가치) |
| 2 | 한국어 + 영어 혼합 쿼리의 임베딩 품질은? | 검색 정확도 | 벤치마크 필요 |
| 3 | 10만+ 파일에서 LanceDB ANN 성능은? | 스케일링 | 프로파일링 필요 |
| 4 | FUSE writeback 지원 범위는? (mv, cp 등) | 사용성 | Phase 4로 연기 |
| 5 | 라이선스: MIT vs Apache 2.0 vs AGPL? | 커뮤니티 전략 | MIT 권장 |

---

## 11. Appendix

### A. Naming Candidates

| 이름 | 느낌 |
|------|------|
| **SemanticFS** | 직관적, 기술적 |
| **MindFS** | 캐치, "생각대로 찾는 파일시스템" |
| **SenseFS** | 짧고 깔끔 |
| **Pathless** | 마케팅적 ("경로가 필요 없다") |
| **Semaphore** | 기존 이름과 충돌 가능 |

### B. Reference Projects

- [FUSE (Filesystem in Userspace)](https://github.com/libfuse/libfuse)
- [fuser (Rust FUSE)](https://github.com/cberner/fuser)
- [LanceDB](https://github.com/lancedb/lancedb)
- [Ollama](https://github.com/ollama/ollama)
- [OpenDocuments (author's prior work)](https://github.com/)

### C. Research References

- "Semantic File Systems" (Gifford et al., 1991) — MIT, 원조 논문
- "Stuff I've Seen" (Dumais et al., 2003) — Microsoft Research
- "TagFS" (Bloehdorn et al., 2006) — 태그 기반 파일시스템
- "CLIP: Learning Transferable Visual Models" (Radford et al., 2021) — 이미지 임베딩

---

*End of Document*s
