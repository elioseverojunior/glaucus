# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### 🚀 Features

- *(core)* Honour the %YAML directive in scalar resolution ([c38b8a1](https://github.com/elioseverojunior/glaucus/commit/c38b8a16867ef20619e10a83965d6f744d38be66))

### 🐛 Bug Fixes

- *(conformance)* Enforce the 100% floor the suite documents ([6c39a71](https://github.com/elioseverojunior/glaucus/commit/6c39a715e090606152246cc54f06d7633a75b823))
- *(ast)* Bound alias materialisation against a node budget ([cd7ffd8](https://github.com/elioseverojunior/glaucus/commit/cd7ffd8a04502813656baa2c5d5c72d255498813))
- *(serde)* Keep bare `inf` / `nan` / `infinity` as strings ([4c983fc](https://github.com/elioseverojunior/glaucus/commit/4c983fcc04a7af4caea6b11aa0ff1012ee2d3d2f))
- *(serde)* Let core-schema tags drive scalar resolution ([f91cecb](https://github.com/elioseverojunior/glaucus/commit/f91cecb2069855acf8a1dd3277790d721ace51cc))
- *(core)* Cap anchor count and anchor-name length ([8b4c23a](https://github.com/elioseverojunior/glaucus/commit/8b4c23a664d568dca46b769b010dad7221d58def))
- *(core)* Give scalar length a bounded default ([d8cbb8f](https://github.com/elioseverojunior/glaucus/commit/d8cbb8fa47f01404d84a657621fc9b6e14a540da))
- *(ast)* Compose empty and comment-only input as a null document ([d33ff4c](https://github.com/elioseverojunior/glaucus/commit/d33ff4ca7ed431ad1ab3cb612d0b01afdbafaaf0))

### 📚 Documentation

- Document the new limits and correct the stated thresholds ([1c31a0d](https://github.com/elioseverojunior/glaucus/commit/1c31a0dca12fa4d18cf2c3f309534eb13210f96c))

### 🧪 Testing

- Restore 100% coverage over the parity series ([44bf713](https://github.com/elioseverojunior/glaucus/commit/44bf713558e1106af7143ecffbbd1a312e732bdb))

### ⚙️ Miscellaneous Tasks

- Update CHANGELOG.md for v0.2.2 [skip ci] ([6ec47be](https://github.com/elioseverojunior/glaucus/commit/6ec47be1007a095a2e3e0aa5b0644e62962fb2fb))

## [0.2.2](https://github.com/elioseverojunior/glaucus/releases/tag/v0.2.2) - 2026-08-22

### 🐛 Bug Fixes

- *(ci)* Guard the labeler checkout by trigger ([5befea2](https://github.com/elioseverojunior/glaucus/commit/5befea2aba1d9e232ae64157a11489841fbd1cbf))
- *(ci)* Publish scorecard from a static runner ([9b5d071](https://github.com/elioseverojunior/glaucus/commit/9b5d0710e8dd34ed2390adaefea1e0f3c6734605))
- *(dependabot)* Group codeql-action updates ([0351d26](https://github.com/elioseverojunior/glaucus/commit/0351d260e21507897b4316a3faab9b38db4e7d78))

### 🚜 Refactor

- *(ci)* Let rust-toolchain own the cargo cache ([0e9448b](https://github.com/elioseverojunior/glaucus/commit/0e9448b22e268c68a8099814ce251e589ae5a882))
- *(ci)* Make codeql.yml the only CodeQL call ([0b67bf7](https://github.com/elioseverojunior/glaucus/commit/0b67bf7850b3d71c5f0b081341ce5c7fe0a89058))

### 📚 Documentation

- Add the project logo to the README ([edc2d86](https://github.com/elioseverojunior/glaucus/commit/edc2d86b8d39628262abcacac4c2d181b8ebf71b))

### ⚙️ Miscellaneous Tasks

- Update CHANGELOG.md for v0.2.1 [skip ci] ([6e8caa3](https://github.com/elioseverojunior/glaucus/commit/6e8caa36dff36e294844bf8e1ce0cab0ff48c74e))
- *(hk)* Bump hk to 1.54.1 ([315cdbc](https://github.com/elioseverojunior/glaucus/commit/315cdbcf115fd07638ee46eb3d5f7fce4b43b153))
- *(runners)* Prefer RUNS_ON_ARM64 when it is set ([183418a](https://github.com/elioseverojunior/glaucus/commit/183418a480c5c6615a5e1da6b609c4576cc452bc))
- *(perms)* Scope every write to the job that needs it ([884a648](https://github.com/elioseverojunior/glaucus/commit/884a6484f54bf08d20ae7fbff5b9707b169699f7))
- *(perms)* Drop read-all in the mise workflow ([8af4cf8](https://github.com/elioseverojunior/glaucus/commit/8af4cf8ad79a4fc283611eadb37f030cf3e33396))
- *(hk)* Bump hk to 1.56.0 ([22ef822](https://github.com/elioseverojunior/glaucus/commit/22ef822843557314561323945032b7c7ec85271a))
- *(mise)* Relock git-cliff on gnu linux assets ([b8030c1](https://github.com/elioseverojunior/glaucus/commit/b8030c154e65b16d54cc2eca7a3df9c9b8718ca6))
- *(deps)* Bump the locked dependency graph ([7c73164](https://github.com/elioseverojunior/glaucus/commit/7c73164dd72b5dde98c646e9c80ae432c58c964d))
- Bump pinned action SHAs to current tags ([ede3b75](https://github.com/elioseverojunior/glaucus/commit/ede3b754ce1471b1a7c460793c4d336cb8fc9d51))
- *(release)* Bump the workspace to 0.2.2 ([07f8f61](https://github.com/elioseverojunior/glaucus/commit/07f8f612a69dd48c400f14497432b34ea7bc739b))

## [0.2.1](https://github.com/elioseverojunior/glaucus/releases/tag/v0.2.1) - 2026-08-10

### 🐛 Bug Fixes

- *(msrv)* Lower rust-version from 1.97 to 1.88 ([e18554d](https://github.com/elioseverojunior/glaucus/commit/e18554df504438221fabe91cd59a091e5d74fe4d))
- *(msrv)* Provision the floor directly ([94f181e](https://github.com/elioseverojunior/glaucus/commit/94f181e0364e8b3f1ff605c92f8bda603da9b24c))
- *(msrv)* Hold clippy to the declared floor ([85919d1](https://github.com/elioseverojunior/glaucus/commit/85919d16abf264560cee50f8d631dea1012608a2))
- *(ci)* Pin the coverage instrumentation nightly ([504d778](https://github.com/elioseverojunior/glaucus/commit/504d77852a062c9ef7e0bed80460724a8560f6b6))
- *(docs)* Align the glaucus requirement with 0.2 ([621a7c7](https://github.com/elioseverojunior/glaucus/commit/621a7c7476db32fba9145477b6766760495e16c6))
- *(mise)* Gate the version restated in markdown ([7e5d13d](https://github.com/elioseverojunior/glaucus/commit/7e5d13da95180337ba8ef2d6e9ecc55c39b106e7))
- *(hk)* Run the version gate on markdown edits ([f66da68](https://github.com/elioseverojunior/glaucus/commit/f66da686689cda3c43ff38b59c22779ce2231098))

### 📚 Documentation

- Refresh the codecov badge token ([1d1f8fe](https://github.com/elioseverojunior/glaucus/commit/1d1f8fed93ce1bfed5ecd2fde9de2dab55a39c6f))

### ⚙️ Miscellaneous Tasks

- Update CHANGELOG.md for v0.2.0 [skip ci] ([4c7f4f6](https://github.com/elioseverojunior/glaucus/commit/4c7f4f6f90c4cc3dd98ceb4f3e39500e7c4ad230))
- *(msrv)* Verify the declared MSRV on every run ([e78f91f](https://github.com/elioseverojunior/glaucus/commit/e78f91f62d0c0785b7b93211173d3ed8b6f26732))
- *(release)* Bump the workspace to 0.2.1 ([a24e855](https://github.com/elioseverojunior/glaucus/commit/a24e8554dfb5c8b01a9ec6fb20123fba609e0ab0))

## [0.2.0](https://github.com/elioseverojunior/glaucus/releases/tag/v0.2.0) - 2026-08-03

### 🚀 Features

- *(ci)* Add a callable changelog workflow ([ff9c4dd](https://github.com/elioseverojunior/glaucus/commit/ff9c4dddf9679e623bfb550920412d2d312445cf))
- *(ci)* Commit CHANGELOG.md after a real publish ([2d2c98e](https://github.com/elioseverojunior/glaucus/commit/2d2c98e3e07737fdff1126fcdc21d526d246a200))

### 🐛 Bug Fixes

- *(ci)* Restrict publishing to release refs ([f10f60f](https://github.com/elioseverojunior/glaucus/commit/f10f60f9df5da81f2ab1eb128383be82874dd7c8))
- *(ci)* Pass dry-run to publish as a boolean ([629c9b0](https://github.com/elioseverojunior/glaucus/commit/629c9b004b934409b9688acb8477647aa6a7325f))
- *(changelog)* Treat only a plain vX.Y.Z as a release ([685c0f1](https://github.com/elioseverojunior/glaucus/commit/685c0f1c8523c0ae8c74ae68a0674992e1e81308))
- *(mise)* Let release:prepare run from a release branch ([8d88df6](https://github.com/elioseverojunior/glaucus/commit/8d88df666dca483e946a0bf7980dcbcbefaef2e8))

### 🚜 Refactor

- *(ci)* Delegate changelog generation to publish ([a63fb17](https://github.com/elioseverojunior/glaucus/commit/a63fb1749411a83c2d9ce6c417fdc753b536beaf))

### 📚 Documentation

- Correct the release flow after moving the changelog ([e6fac8b](https://github.com/elioseverojunior/glaucus/commit/e6fac8b01bb15e19a1d360a1ccc1432924d5df2f))

### ⚙️ Miscellaneous Tasks

- Update CHANGELOG.md for 0.1.1 [skip ci] ([25aa4d8](https://github.com/elioseverojunior/glaucus/commit/25aa4d8496c35c2df6b272c7e51e59c9985e167c))
- *(reuse)* Exempt generated files from header checks ([f3b67ef](https://github.com/elioseverojunior/glaucus/commit/f3b67ef660ff37f7fc299aa122c82d2ce79e4182))
- *(release)* Bump the workspace to 0.2.0 ([7234cfb](https://github.com/elioseverojunior/glaucus/commit/7234cfbe2953504ef7ee564da45956844e20c46f))

## [0.1.1](https://github.com/elioseverojunior/glaucus/releases/tag/v0.1.1) - 2026-08-03

### 🐛 Bug Fixes

- *(docs)* Repoint dead nav links at real pages ([e0784a7](https://github.com/elioseverojunior/glaucus/commit/e0784a75d0300dbc089b9bc18b9d824e2b928713))
- *(mise)* Retarget comply references at glaucus ([866979a](https://github.com/elioseverojunior/glaucus/commit/866979a17ca423ae96cecac454f4a4394b2245f1))
- *(version)* Derive release from MajorMinorPatch ([9d4be82](https://github.com/elioseverojunior/glaucus/commit/9d4be82799dd1bbb7fcc65489b25d55ba57da334))
- *(ci)* Confirm release tag against GitVersion ([454427d](https://github.com/elioseverojunior/glaucus/commit/454427d12bd9b39bbeb04439c5701e1c374f52a6))
- *(changelog)* Accept non-numeric prerelease tags ([13b2b34](https://github.com/elioseverojunior/glaucus/commit/13b2b347ecddb2aad9e81e1bc69ca1c07d43f5cc))
- *(ci)* Generate CHANGELOG.md in the release pipeline ([b8a3feb](https://github.com/elioseverojunior/glaucus/commit/b8a3febeb1b9a99c57538321a1d6798f06a30ff5))

### 📚 Documentation

- Document the release tagging scheme ([7ed51ec](https://github.com/elioseverojunior/glaucus/commit/7ed51ec59b41c6b8c1b942eb07f1b20c4402823d))
- Drop the completed refactoring scope note ([217af7b](https://github.com/elioseverojunior/glaucus/commit/217af7b93a7ac31ce87d5caf2d3eeca158245cc8))

### ⚡ Performance

- *(ci)* Start build alongside lint and sast ([e6b3d61](https://github.com/elioseverojunior/glaucus/commit/e6b3d618c608b68cf9c791d294f48746eced97f5))

### 🧪 Testing

- *(mise)* Add conformance task and split test ([8ea8959](https://github.com/elioseverojunior/glaucus/commit/8ea89598445369cd07f86cf90d4f835e9b69aad3))
- *(ci)* Assert the stable-version contract ([9b95e34](https://github.com/elioseverojunior/glaucus/commit/9b95e344e94cdd9ff48ba1932c271c7af04d3a19))

### ⚙️ Miscellaneous Tasks

- *(mise)* Init submodules for setup+worktree ([5c7e162](https://github.com/elioseverojunior/glaucus/commit/5c7e16284a74a828c389c3ab2eb2551ad7fac58e))
- *(release)* Bump the workspace to 0.1.1 ([330599e](https://github.com/elioseverojunior/glaucus/commit/330599ec6ecda4d6d9e225c1dd670e271f962503))

## [0.1.0](https://github.com/elioseverojunior/glaucus/releases/tag/v0.1.0) - 2026-08-03

### 🎉 Repository Initialization

- Repository initialization ([abcee30](https://github.com/elioseverojunior/glaucus/commit/abcee305478276230eb9d45fdbb1b37f24aec10d))

<!-- generated by git-cliff -->

[unreleased]: https://github.com/elioseverojunior/glaucus/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/elioseverojunior/glaucus/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/elioseverojunior/glaucus/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/elioseverojunior/glaucus/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/elioseverojunior/glaucus/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/elioseverojunior/glaucus/compare/v0.0.1...v0.1.0
