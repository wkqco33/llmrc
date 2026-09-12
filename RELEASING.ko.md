# llmrc 릴리스 가이드

릴리스는 태그로 트리거됩니다. `[workspace.package].version`이 유일한 버전
기준이며(11개 크레이트가 모두 이를 상속합니다), 일치하는 태그를 푸시하면
`.github/workflows/publish.yml`이 crates.io로 배포합니다.

> English: [RELEASING.md](RELEASING.md)

---

## 최초 1회 설정

1. **crates.io 토큰.** <https://crates.io/settings/tokens>에서
   `publish-new`와 `publish-update` 스코프를 가진 API 토큰을 만들고,
   저장소 시크릿 `CARGO_REGISTRY_TOKEN`으로 등록합니다.

   ```bash
   tr -d '\r\n' < crates_token.txt \
     | gh secret set CARGO_REGISTRY_TOKEN --repo wkqco33/llmrc
   ```

   `wkqco33/llmrc`에는 이미 등록되어 있습니다. 토큰을 절대 커밋하지 마세요.
   작업 트리는 `crates_token.txt`와 `*_token.txt`를 무시하도록 설정되어 있습니다.

2. **선택: 릴리스 승인 게이트.** 저장소 설정에서 `crates-io` 환경을 만들고
   필수 리뷰어를 지정하세요. 배포 작업이 이 환경을 대상으로 하므로, 업로드 전에
   승인이 필요해집니다.

---

## 배포 순서

패키징된 크레이트는 자신의 path 의존성을 crates.io 인덱스에서 해석하므로,
의존하는 모든 크레이트가 먼저 배포되어 있어야 합니다.
`scripts/publish-crates.sh`가 이 순서를 강제합니다.

| # | 크레이트 | 의존 대상 |
| --- | --- | --- |
| 1 | `llmrc-core` | — |
| 2 | `llmrc-runtime` | core |
| 3 | `llmrc-agent` | core, runtime |
| 4 | `llmrc-openai` | core |
| 5 | `llmrc-ollama` | core |
| 6 | `llmrc-mcp` | core, agent |
| 7 | `llmrc-bots-core` | agent |
| 8 | `llmrc-bots-discord` | bots-core |
| 9 | `llmrc-bots-telegram` | bots-core |
| 10 | `llmrc-bots-slack` | bots-core |
| 11 | `llmrc` (파사드) | 위 전체 |

각 배포 후 스크립트는 새 버전이 스파스 인덱스에 나타날 때까지 폴링하므로,
의존 크레이트가 인덱스와 경쟁하지 않습니다.

---

## crates.io 속도 제한

공식 [publish 속도 제한](https://crates.io/docs/rate-limits) 기준:

| 배포 유형 | 버스트 | 지속 속도 |
| --- | --- | --- |
| 완전히 새로운 크레이트 | 한 번에 5개 | 이후 10분마다 1개 |
| 기존 크레이트의 새 버전 | 한 번에 30개 | 이후 1분마다 1개 |

따라서 **최초** 릴리스가 느립니다. 새 크레이트 11개는 5개 버스트 이후 6개를
약 10분 간격으로 배포하므로 **약 1시간**이 걸립니다. 스크립트는 crates.io가
알려주는 재시도 시각을 읽어 자동으로 대기하며, 작업 타임아웃은 180분입니다.

이후 릴리스는 이미 존재하는 크레이트의 새 버전을 배포하므로 30개 버스트에
해당하여, 워크스페이스 전체가 한 번에 나갑니다.

---

## 릴리스 절차

```bash
# 1. 단일 버전 필드를 올립니다.
$EDITOR Cargo.toml        # [workspace.package] version = "0.2.0"

# 2. 의존성이 바뀌었다면 lockfile을 갱신합니다.
cargo check --workspace --all-features

# 3. 품질 게이트를 실행합니다.
cargo fmt --all -- --check
cargo check --workspace --all-features --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-features --all-targets --locked -- -D warnings

# 4. 커밋한 뒤 일치하는 버전으로 태그를 만듭니다.
git commit -am "release: 0.2.0"
git tag -a v0.2.0 -m "v0.2.0"
git push origin master
git push origin v0.2.0
```

태그를 푸시하면 `Release` 워크플로가 시작됩니다.

1. `verify` — rustfmt, clippy, 전체 테스트 스위트.
2. `version` — 태그가 `[workspace.package].version`과 다르면 즉시 실패.
3. `publish` — `CARGO_REGISTRY_TOKEN`으로 `scripts/publish-crates.sh`를 실행하고,
   자동 생성된 노트로 GitHub 릴리스를 만듭니다.

---

## 드라이 런

아무것도 업로드하지 않고 패키징만 검증합니다.

```bash
DRY_RUN=1 scripts/publish-crates.sh
```

또는 `Release` 워크플로를 수동 실행(Actions → Release → Run workflow)하면서
`dry_run`을 `true`로 두면 됩니다.

전체 `cargo publish --dry-run`은 의존 크레이트가 이미 crates.io에 있을 때만
동작합니다. 따라서 드라이 런 모드는 의존성이 없는 크레이트를 끝까지 검증하고,
나머지는 매니페스트와 파일 목록을 검사합니다.

---

## 중단된 릴리스 이어하기

스크립트는 멱등합니다. 정확한 버전이 이미 스파스 인덱스에 있는 크레이트는
건너뛰므로, 속도 제한·네트워크 오류·작업 취소로 릴리스가 중간에 멈춰도 다시
실행하면 첫 미배포 크레이트부터 이어집니다.

Actions UI에서 실패한 작업을 재실행하거나(같은 태그, 같은 커밋), 로컬에서:

```bash
CARGO_REGISTRY_TOKEN=... scripts/publish-crates.sh
```

---

## 릴리스 확인

- 크레이트 페이지: <https://crates.io/crates/llmrc>, `llmrc-core` 등.
- 문서: docs.rs 빌드가 끝나면 <https://docs.rs/llmrc>.
- 워크플로가 만든 GitHub 릴리스.

---

## 배포 후 문제가 있을 때

배포된 버전은 삭제할 수 없고 yank만 가능합니다.

```bash
cargo yank --version 0.2.0 -p llmrc-core
```

yank는 기존 lockfile을 깨뜨리지 않으면서 새 의존 사용자를 경고합니다. 결함이
있다면 해당 크레이트를 yank하고 평소 태그 흐름으로 패치 버전을 배포하세요.

---

## 워크플로 보안 강화 메모

- 모든 액션은 전체 길이 커밋 SHA로 고정했습니다.
- `actions/checkout`은 `persist-credentials: false`로 실행됩니다.
- 릴리스 워크플로는 의도적으로 **빌드 캐시를 사용하지 않습니다**. crates.io
  토큰에 접근할 수 있는 유일한 워크플로이고, 오염된 캐시는 공급망 공격 경로이기
  때문입니다. 권한이 없는 CI는 캐시를 사용합니다.
