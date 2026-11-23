[English](README.md) | **한국어** | [日本語](README.ja.md) | [简体中文](README.zh-CN.md)

# bmo-drive

자체 API를 갖춘 하나의 서비스로 된 파일 뷰어입니다. 관리자는 이 머신의 드라이브, Garage, 노드의 볼륨을 장소로
탐색합니다. 각 소유자는 할당량이 있는 자신만의 BMO 드라이브를 가지며, 그 안의 내용을 읽고 바꿀 수 있습니다. 토큰이
없는 사람에게 파일이나 폴더 하나를 건네는 공유 링크와, 읽는 사람의 회사 구성원이 그것을 읽을 수 있게 하는 구성원
공유도 제공합니다. 바이너리에는 서비스와 함께 명령 `bmo-drive file share`, 클라이언트, 하니스 플러그인(MCP 서버
`bmo-drive mcp`와 `bmo-drive` 스킬)이 들어 있습니다. 다른 모든 유닛에는 `docs/api.md`에 나열된 대로 각자의 API를
통해 접근합니다.

## 테스트

테스트베드의 `personal-data` 검사가 개인정보 규칙(`PERSONAL-DATA.md`)을 지킵니다. 저장소에는 다음 테스트가
있습니다. `tests/import.rs`는 이전 아카이브의 공유 링크 가져오기를, `tests/owners.rs`는 소유자와 두 갈래를
검증합니다. 각 소유자의 드라이브는 그 소유자만의 것이고, 할당량은 바이트가 도착하기 전에 판단되며, 머신은
관리자만의 것이고, 공유 링크는 두 갈래 모두에서 열립니다. `tests/members.rs`는 구성원 공유를 검증합니다. 구성원과
공유한 파일이나 폴더는 그 구성원만 읽을 수 있고, 공유가 끝나면 다시 읽기가 거부됩니다. `tests/console.rs`는 콘솔
페이스를 검증합니다. 서비스가 제공하는 매니페스트를 `client/vendor/`에 있는 릴리스에 대해 bmo-ui 자체의 읽기
방식으로 읽고(bmo-console-face가 Bun으로 실행), 페이스가 콘솔의 자격 증명에 연결되어 있는지, 그 밖의 모든 경로를
드라이브 자체가 보호하는지 확인합니다. `tests/links.rs`는 볼륨 위의 링크를 검증합니다. 자기 자신으로 되돌아가는
경로는 거부되고, 옆에 있는 폴더로의 링크는 한 번만 탐색되며, 각 링크의 대상이 목록에 나타나고, 폴더의 zip은 링크에서
끝납니다. `tests/screen.rs`는 Files 화면을 검증하며, 화면의 모듈도 같은 방식으로 Bun에서 실행합니다. 화면이 어디서
열리는지, 장소 기억, 소유자에게 공유된 항목의 갈래, 그리고 각 공유가 구성원 자신의 경로로 읽히는 읽기 전용 장소인지를
확인합니다. `cargo test`가 이 모두를 실행합니다. 번들은 브라우저 테스트를 `client/tests/`에 둡니다. Files 화면은
그곳의 작은 Nuxt 호스트 안에서 단독으로 서고, 번들 자체의 문구와 함께 개발 서버로 제공되며, 서비스의 대역을 상대로
Chromium에서 구동됩니다. `tests/browser/files-upload.test.ts`는 열린 폴더에 파일을 끌어다 놓거나 붙여 넣고,
붙여 넣는 순간 브라우저가 이름을 붙이는 그림을 붙여 넣고, 이름 바꾸기 입력란에 붙여 넣고, 화면에서 복사한 항목을
붙여 넣습니다. `tests/browser/files-view.test.ts`는 넓은 창과 휴대폰 너비에서 도구 모음으로 보기를 고릅니다. 번들의
검사인 `client/` 안의 `bun run check`(고정된 설치, 타입 검사, 브라우저 테스트)가 이들과 나란히 있습니다.
`docs/inventory.md`는 이전 동작을 지키던 테스트를 나열합니다.

## 구성

| 폴더 | 내용 |
|---|---|
| `src/` | 서비스, 명령, 클라이언트, MCP 서버. 하나의 크레이트를 각각의 모듈로 나눈 것 |
| `migrations/` | 저장소. 테이블을 순서대로 만드는 구문 |
| `skills/` | `bmo-drive` 스킬. MCP 서버와 함께 하니스 플러그인의 나머지 절반 |
| `client/` | 콘솔 플러그인 번들: Files 화면과 파일 공유 페이지. 독립된 프로젝트이며 빌드는 `client/console/`에 있음 |
| `deploy/` | 서비스의 사용자 유닛 |
| `docs/` | 서비스가 하는 일에 대한 정식 기술과 OpenAPI 문서 |
| `tests/` | 지금까지 갖춰진 동작 테스트 |

루트에는 Cargo 매니페스트, `VERSION`, `testbed.toml`, `install.toml`, `rustfmt.toml`, `CLAUDE.md`, `PERSONAL-DATA.md`,
`korean-ledger.json`, `settings.toml`, `settings.local.toml.example`(배포가 제공하는 로컬 설정의 예시),
`LICENSE`가 있습니다.

## 콘솔 플러그인 번들

`client/`에는 bmo-ui 위에 만든 Files 화면과 파일 공유 페이지가 있습니다. bmo-ui의 콘솔 플러그인 계약(bmo-ui의
`docs/console-bundles.md`)을 따르는 번들이며, 그 안에서 이 서비스가 정한 내용은 `docs/api.md`의 「The console
face」에 있습니다. 번들의 문구는 자체 언어 팩인 `client/i18n/`에 로케일마다 파일 하나씩, `drive.*` 아래에 있고,
`docs/api.md`의 각 거부 코드마다 `drive.error.<code>`가 있습니다.

- **빌드.** `bun run bundle`은 bmo-ui의 번들 빌드로 화면 소스에서 `client/dist/`를 만듭니다. 이 빌드는 콘솔이
  공유하는 라이브러리를 번들에 넣지 않고, 번들이 기반으로 하는 레이어를 씁니다. 결과물은 두 페이지의 라우트를 기본
  export로 내보내는 모듈 `bundle.js`, 화면이 레이어의 토큰 위에서 쓰는 유틸리티 클래스를 담은 스타일시트
  `bundle.css`, 로케일을 키로 하는 하나의 객체로 된 언어 팩 `words.json`, 그리고 `layer.json`입니다. 페이지가
  import 없이 쓰는 컴포넌트와 Vue·Nuxt 함수는 빌드에서 import되며, 빌드는 `client/console/`에 있습니다. 배포는
  릴리스마다 이 폴더를 빌드하고, 서비스는 매니페스트와 함께 그 파일들을 제공합니다.
- **레이어.** `client/vendor/`에는 bmo-ui의 릴리스 명령이 쓴 그대로의 bmo-ui 릴리스 파일이 있으며, bmo-chat의
  `shared-versions.toml`이 고정한 바로 그 파일입니다. 번들은 이를 기준으로 설치, 빌드, 타입 검사됩니다. 컴포넌트
  라이브러리는 그 릴리스가 지정한 정확한 버전으로 고정되고, 설치 시 레이어 자체의 패치(프리미티브와 아이콘 포함)가
  적용됩니다. 패치는 `client/patches/`에 있으며, 각 패치는 자신이 지정한 정확한 버전에만 적용됩니다. `client/`의
  Nuxt 설정은 타입 검사에만 쓰입니다.
- **타입.** 서비스 응답에 대한 화면의 타입은 `bun run api`로 `docs/openapi.json`에서 `client/shared/openapi.ts`로
  생성됩니다.

## 문서

| 문서 | 내용 |
|---|---|
| `docs/files.md` | 파일 뷰어: 머신, 장소, 울타리, 읽기, 변경, 보호된 위치, 공유 링크, 구성원 공유 |
| `docs/api.md` | 라우트, 응답과 거부, 이벤트, 명령, 하니스 플러그인, 클라이언트, 다른 유닛 호출, 설정 |
| `docs/openapi.json` | 같은 계약의 기계 판독용 형태. `bmo-drive openapi`로 생성 |
| `docs/inventory.md` | 이 저장소가 이전 저장소에서 가져온 것. 각 묶음의 개수와 출처 |
| `PERSONAL-DATA.md` | 이 저장소에 절대 있어서는 안 되는 것(사람의 이름, 주소, 경로, 기기나 네트워크 정보, 방 구성원이나 회사의 정보, 실제 대화의 한 줄)과 이를 지키는 검사 |

## 버전

`VERSION`은 저장소의 버전이자 크레이트 자체의 버전을 담습니다. 버전이 어떻게 바뀌는지는 `docs/api.md`의
「Versions」에 있습니다.

## 라이선스

MIT. 전문은 `LICENSE`에 있습니다.
