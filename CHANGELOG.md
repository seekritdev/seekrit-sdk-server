# Changelog

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
