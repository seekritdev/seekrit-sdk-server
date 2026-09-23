# Changelog

## [0.7.4](https://github.com/mileszim/seekrit/compare/sdk-server-v0.7.3...sdk-server-v0.7.4) (2026-09-23)


### Dependencies

* **deps:** bump aes-gcm from 0.10.3 to 0.11.1 in /crates/seekrit-core ([#454](https://github.com/mileszim/seekrit/issues/454)) ([827555e](https://github.com/mileszim/seekrit/commit/827555ea0bdbd5ac84b0be0634182bbec2e220c4))

## [0.7.3](https://github.com/mileszim/seekrit/compare/sdk-server-v0.7.2...sdk-server-v0.7.3) (2026-09-16)


### Dependencies

* **deps:** bump the rust group across 2 directories with 3 updates ([#421](https://github.com/mileszim/seekrit/issues/421)) ([d21b7bf](https://github.com/mileszim/seekrit/commit/d21b7bf407b54c92c0552ad713d290712528111a))

## [0.7.2](https://github.com/mileszim/seekrit/compare/sdk-server-v0.7.1...sdk-server-v0.7.2) (2026-09-09)


### Dependencies

* **deps:** bump axum from 0.7.9 to 0.8.9 in /apps/seekrit-sdk-server ([#338](https://github.com/mileszim/seekrit/issues/338)) ([5d5528c](https://github.com/mileszim/seekrit/commit/5d5528c706a4c966af58985adce9a2c92e8a7a61))
* **deps:** bump sha2 from 0.10.9 to 0.11.0 in /crates/seekrit-cache ([#366](https://github.com/mileszim/seekrit/issues/366)) ([214a83a](https://github.com/mileszim/seekrit/commit/214a83a5223e3061b41fb58b120de7e7f9c0191f))
* **deps:** bump sha2 from 0.10.9 to 0.11.0 in /crates/seekrit-core ([#367](https://github.com/mileszim/seekrit/issues/367)) ([24ae136](https://github.com/mileszim/seekrit/commit/24ae136559a3f60eddc31d5797cb3ab64a8c8742))

## [0.7.1](https://github.com/mileszim/seekrit/compare/sdk-server-v0.7.0...sdk-server-v0.7.1) (2026-09-09)


### Dependencies

* **rust:** sync the app lockfiles with seekrit-core ([#360](https://github.com/mileszim/seekrit/issues/360)) ([eb36425](https://github.com/mileszim/seekrit/commit/eb3642520bb149621078382553263a7a8845e06d))

## [0.7.0](https://github.com/mileszim/seekrit/compare/sdk-server-v0.6.0...sdk-server-v0.7.0) (2026-08-19)


### Features

* **agents:** agent access policy, signed in the browser ([#231](https://github.com/mileszim/seekrit/issues/231)) ([d15f092](https://github.com/mileszim/seekrit/commit/d15f0926a76c8b9dbe58bedd2436bd7f25ea0e28))

## [0.6.0](https://github.com/mileszim/seekrit/compare/sdk-server-v0.5.1...sdk-server-v0.6.0) (2026-08-15)


### Features

* opt-in last-known-good cache for the integration tools ([#192](https://github.com/mileszim/seekrit/issues/192)) ([c14eeaa](https://github.com/mileszim/seekrit/commit/c14eeaa3c01f9d397e71033ecc13d7e747e0ef25))

## [0.5.1](https://github.com/mileszim/seekrit/compare/sdk-server-v0.5.0...sdk-server-v0.5.1) (2026-08-15)


### Bug Fixes

* **build:** bump Rust image to 1.97 so the OTel lockfiles build ([#190](https://github.com/mileszim/seekrit/issues/190)) ([f544ff2](https://github.com/mileszim/seekrit/commit/f544ff2eb272506f8a0e9e7558f85690f3682ffe))

## [0.5.0](https://github.com/mileszim/seekrit/compare/sdk-server-v0.4.0...sdk-server-v0.5.0) (2026-08-15)


### Features

* OpenTelemetry for the self-hosted services ([#187](https://github.com/mileszim/seekrit/issues/187)) ([ead4ac6](https://github.com/mileszim/seekrit/commit/ead4ac6492e2e032e0ad0c25f0fbbf7830397a8a))

## [0.4.0](https://github.com/mileszim/seekrit/compare/sdk-server-v0.3.0...sdk-server-v0.4.0) (2026-07-25)


### Features

* secret references — expand ${OTHER_SECRET} at read time ([#156](https://github.com/mileszim/seekrit/issues/156)) ([d17adab](https://github.com/mileszim/seekrit/commit/d17adab6198da3b155083a607e974317bcd54b91))

## [0.3.0](https://github.com/mileszim/seekrit/compare/sdk-server-v0.2.0...sdk-server-v0.3.0) (2026-07-19)


### Features

* **kms:** AWS KMS-compatible gateway (seekrit-kms) ([#108](https://github.com/mileszim/seekrit/issues/108)) ([e25e535](https://github.com/mileszim/seekrit/commit/e25e53537b282d217abcabb924e6a1464e361b30))

## [0.2.0](https://github.com/mileszim/seekrit/compare/sdk-server-v0.1.0...sdk-server-v0.2.0) (2026-07-18)


### Features

* **k8s:** External Secrets Operator integration (sidecar + Helm chart) ([#85](https://github.com/mileszim/seekrit/issues/85)) ([7106968](https://github.com/mileszim/seekrit/commit/710696885c29e940b2d1ceccff5e36b113057d04))
