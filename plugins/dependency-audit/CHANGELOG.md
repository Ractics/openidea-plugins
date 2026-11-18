# Changelog

## [0.4.0](https://github.com/arnaud-ritti/openidea-plugins/compare/dependency-audit-v0.3.0...dependency-audit-v0.4.0) (2026-11-18)


### Features

* **dependency-audit:** make the npm lockfile graph size limit a setting ([544f155](https://github.com/arnaud-ritti/openidea-plugins/commit/544f155d026399776b6d9be39f187562bbec2ae7))
* **dependency-audit:** read Maven/Gradle versions from the IDE's resolved dependency graph ([8cfe897](https://github.com/arnaud-ritti/openidea-plugins/commit/8cfe8977112c8f45f32bcd2e4ae490eaa9594c38))
* **dependency-audit:** vulnerability notes and inline warnings, diff before upgrade, no automatic build runs by default ([cc1f4c1](https://github.com/arnaud-ritti/openidea-plugins/commit/cc1f4c1a13e40d330ba2c5c264dd1b21694eb587))
* **ui-kit:** grouped hub sections, accessible grids, error and permission states ([de9e5ab](https://github.com/arnaud-ritti/openidea-plugins/commit/de9e5abc728029e5875e942bdbf6ae931f193cdf))


### Bug Fixes

* consistent command titles and spacing in messages ([86ecc8d](https://github.com/arnaud-ritti/openidea-plugins/commit/86ecc8d4f27bd0b4ed2eb1b2f7ee68cd5bd76374))
* correct manifest mismatches and unsafe defaults ([d555f8d](https://github.com/arnaud-ritti/openidea-plugins/commit/d555f8d68e191d63e2b0d69e5f52bcdc26561c0d))
* **dependency-audit:** filter private npm v1 lock entries and list poetry source packages ([a17e917](https://github.com/arnaud-ritti/openidea-plugins/commit/a17e917c89d068a249c6a3b4da76c508ed209ff2))
* **dependency-audit:** keep pnpm private-tarball packages local; narrow privacy note to filtered lockfiles ([e37c8b2](https://github.com/arnaud-ritti/openidea-plugins/commit/e37c8b276b06e91df0e3e2a48818540be8aafb44))
* **dependency-audit:** map tool denials, report auto-resolve failures, plain names ([b5fa17b](https://github.com/arnaud-ritti/openidea-plugins/commit/b5fa17ba2d832f3c0c4b9eabc0264a23e904ea32))
* **dependency-audit:** narrow pnpm private-package change-note ([c6e7701](https://github.com/arnaud-ritti/openidea-plugins/commit/c6e7701799992d5e1dcb9b983f33281144b634d1))
* **dependency-audit:** no build-resolution warning in projects without Maven or Gradle ([64b73b7](https://github.com/arnaud-ritti/openidea-plugins/commit/64b73b756632e151af208d660cd148efb36eb8f7))
* **dependency-audit:** parse uv/poetry lockfiles correctly and skip private-registry packages ([4311da1](https://github.com/arnaud-ritti/openidea-plugins/commit/4311da136c367b30287540f7de82e9f008f50d92))
* **dependency-audit:** safer defaults, shared-property fixes, Gradle and PEP 440 parsing ([60220a4](https://github.com/arnaud-ritti/openidea-plugins/commit/60220a44dc7d87996ef4fc649574da2dd6f4638b))
* **dependency-audit:** say when no dependency files exist, readable scan times, palette fallback for row commands ([8ef15ea](https://github.com/arnaud-ritti/openidea-plugins/commit/8ef15ea3f71322eaf4c4049031afe12bd9227ace))
* **dependency-audit:** shorten change-notes, let explicit resolve undo a stored no ([24bb87d](https://github.com/arnaud-ritti/openidea-plugins/commit/24bb87d636800e6f44b553643fffad75a5b37a6f))
* **laravel:** register the new UI message keys in the shared catalog ([8e02c5e](https://github.com/arnaud-ritti/openidea-plugins/commit/8e02c5e95861eb4825103eb1895a00cacdee00d4))
* one name per action and plain messages ([5f524dd](https://github.com/arnaud-ritti/openidea-plugins/commit/5f524dd4c3ef2fb1c3e4913cdf56451d3c2e6fbe))
* regenerate UI config and finish message spacing ([6562555](https://github.com/arnaud-ritti/openidea-plugins/commit/6562555a205b3452c0f2fb00c20c672947cb7dd4))
* **spring-text:** share one XML walker, match drive letters case-insensitively ([61d354d](https://github.com/arnaud-ritti/openidea-plugins/commit/61d354d7122df0117b830a25290e76ceaa59d04f))
* **symfony:** run Send Request and database probes without blocking, trim console polls ([5ec5053](https://github.com/arnaud-ritti/openidea-plugins/commit/5ec50534a28e4e81d1365846c85a0b7f23e5dce1))
* tell users which permission to allow (part 1) ([a9565c7](https://github.com/arnaud-ritti/openidea-plugins/commit/a9565c76d8a6fa0e41a656082d635bb5c62b4556))
* **ui-kit:** plain action errors, named plugin settings, shallow docks ([064946a](https://github.com/arnaud-ritti/openidea-plugins/commit/064946a3c8091f7ce5fd6e784d9dbaf0cf728279))
* **ui-kit:** spell the row action Copy as cURL in every plugin ([7e54b13](https://github.com/arnaud-ritti/openidea-plugins/commit/7e54b137cdbc637cb23c538933661b45ef337db1))

## [0.3.0](https://github.com/arnaud-ritti/openidea-plugins/compare/dependency-audit-v0.2.0...dependency-audit-v0.3.0) (2026-11-16)


### Features

* adopt API 1.4 and extend scoped plugin workflows ([5a8a92b](https://github.com/arnaud-ritti/openidea-plugins/commit/5a8a92b6a6268552c45d1e67b8ae40866a25daee))
* adopt plugin API 1.5 (net:ask, extension points, run-target validation, project notifications) ([08db51a](https://github.com/arnaud-ritti/openidea-plugins/commit/08db51a54c99b3b4ae414ab790be2a0783add3c7))
* close another residual round across Dependency Audit, OpenAPI, Symfony and Twig ([2b8dfe8](https://github.com/arnaud-ritti/openidea-plugins/commit/2b8dfe818985ee6378430960ab13eea92a733dd9))
* close bounded Dependency Audit, GitLab, Ignore, Kubernetes and OpenAPI residual slices ([88748a2](https://github.com/arnaud-ritti/openidea-plugins/commit/88748a26fac2cebe07889ba68263889b4573fde0))
* close follow-up residuals in Dependency Audit, GitLab, OpenAPI and Kubernetes plugins ([15e91cc](https://github.com/arnaud-ritti/openidea-plugins/commit/15e91cc2ac526df01a84fbec13afa7e661baad0e))
* close round 5 gaps (Quarkus saved sources, Shell rename, Symfony go-to links, lockfile parsers, Initializr Maven) ([39404e6](https://github.com/arnaud-ritti/openidea-plugins/commit/39404e6b3b539c78586060bf4bedd249ba95e9a7))
* close six small non-Laravel parity residuals ([7fabc9c](https://github.com/arnaud-ritti/openidea-plugins/commit/7fabc9ca01e86306c5596c64c04df72a59caa6c7))
* **dependency-audit:** update hub UI, resolution and call analysis ([2c25bc6](https://github.com/arnaud-ritti/openidea-plugins/commit/2c25bc6f9801c6cb681ec134eef6a8e3a1f5ce51))
* finish small residuals (audit rows, escaped scalar rename, gateway arity) ([3e5c69f](https://github.com/arnaud-ritti/openidea-plugins/commit/3e5c69f197def48cc05526766fc88421536de5d5))
* implement plugin parity workflows and scoped refactors ([d0e1efc](https://github.com/arnaud-ritti/openidea-plugins/commit/d0e1efcd3f7017bb4ebfba8b50901d8affe0e8ba))
* import-level vulnerability evidence and read-only localhost JMX snapshot ([6de8359](https://github.com/arnaud-ritti/openidea-plugins/commit/6de83599fa9520b7184f1ca1796ea803ed7a8c30))
* Maven BOM metadata, oasdiff compat rules, Docker ARG scope, form/column checks, workspace chains, JAX-RS checks, view names, wiring registrations ([17b4876](https://github.com/arnaud-ritti/openidea-plugins/commit/17b48765dc99e46aa52915c5a4139d00a66a0ce2))
* **ui:** consolidate plugin panels and native service actions ([16bbb3e](https://github.com/arnaud-ritti/openidea-plugins/commit/16bbb3eba4999c7dbd0b2d1215ee3db3116ae4c3))


### Bug Fixes

* verification pass against real sources (Yarn berry parser, WS void rule, CRD nulls, Twig falsy namespaces, GitLab draft positions) ([00b86ec](https://github.com/arnaud-ritti/openidea-plugins/commit/00b86ec4f920a18ef709a65a137ff09788186619))

## [0.2.0](https://github.com/arnaud-ritti/openidea-plugins/compare/dependency-audit-v0.1.0...dependency-audit-v0.2.0) (2026-11-07)


### Features

* **dependency-audit:** adopt OpenIDEA API 1.2 and add plugin icon ([d704bb7](https://github.com/arnaud-ritti/openidea-plugins/commit/d704bb7c8c48553940e6c2e974a1f4b710fca3af))
* **dependency-audit:** implement the open audit rows of the v2 pass ([3907ace](https://github.com/arnaud-ritti/openidea-plugins/commit/3907ace880bf488bcab84b58cede3e8454d30088))
* OpenIDEA API 1.2 audit rows, window consolidation, route list, icons ([#5](https://github.com/arnaud-ritti/openidea-plugins/issues/5)) ([26a4dd9](https://github.com/arnaud-ritti/openidea-plugins/commit/26a4dd95b57cc4be7fb61065e33a8c6d56deecea))


### Bug Fixes

* declare the package icon in 24 plugin manifests ([770a781](https://github.com/arnaud-ritti/openidea-plugins/commit/770a781805b051c263c839479e8064cf317c30b9))
* **dependency-audit:** audit every library sharing the version the quick fix bumps ([8f20e40](https://github.com/arnaud-ritti/openidea-plugins/commit/8f20e40de25835bd8559a3c6b870eeb243e9802d))
