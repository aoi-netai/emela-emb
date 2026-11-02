# 旧compilerコードの採用元

基準revision: `c34c0aeefdd966d854371b3110db00a47798751d`。旧checkoutの作業差分を参照せず、すべて `git show <revision>:<path>` から採用した。通常ビルド・生成・試験は旧checkoutを参照しない。

元パッケージ: `crates/emela-codegen-emb`。Rust/Cコードは元workspaceのApache-2.0ライセンスに従う。基準revisionの `LICENSE` とこのrepoの [LICENSE](../LICENSE) はbyte一致（SHA-256: `5df65efbea284e97cad56bd9f5379f985a930c4681260409badce16e29fc1e35`）。元コードの著作権・出自コメントを保持する。Unicode 18.0.0の生成tableと公式vectorは各ファイルのUnicode著作権表示・Terms of Useを保持する。

## 採用範囲と変更

- `contract/`・`semantics/`・`compiler/plan/`・`compiler/emit/`・`writer.rs` とartifact manifestの純粋処理を採用。`compiler/`を`backend/`へ移動し内部pathを変更した。
- upstream adapter、Core/Flowのcrate・入力API、boards・platform製品・CLI・BSPは採用しない。lowerはこのrepoが所有する型付きProgramを検証し、位置・到達性・自己末尾再帰のmetadataを構築する新実装。
- 明示的なC export名をFunctionに追加し、既存の公開symbolを維持する。Outputには既存API用の`c_source`/`header`を保持し、複数ファイルとexportsも返す。
- Rust 1.85を維持するため、旧backendのlet-chainを同じpatternとguardを持つmatchへ置き換えた。Unicodeの`is_multiple_of`は剰余比較へ置き換えた。
- 後続段階用のcontract variantが未使用のため、contract/semanticsでdead_codeを許容。未接続構造をlowerで拒否し、後段コードの採用をsource機能の完成として扱わない。
- Unicode公式vectorと旧backendの独立fixture/heap実行試験を採用。これはP1ソース対応全体の証明ではない。

## Cargo依存

| 依存 | 用途 |
| --- | --- |
| `la-arena 0.3` | backendが所有する型付きIDとArena。上流crate非依存 |
| `serde 1` (`derive`) | artifact schema 2の構造化データ |
| `serde_json 1` | artifact manifestのJSON読み書き |

旧sha2はplatform製品のfingerprint用なので採用しない。間接依存はCargo.lockに固定する。

## 元ファイル一覧

以下のSHA-256は移植元の固定revision内容。新pathは`src/compiler/`を`src/backend/`へ置換し、crate名を`emela-emb-compiler`へ置換したもの。適応変更後のhashではない。

| 元path（`crates/emela-codegen-emb/`以下） | SHA-256 |
| --- | --- |
| `src/artifact.rs` | `91b09f435a27ae47ef07545328fdde204454db2a7b4c2dc16dd246ccf1647973` |
| `src/compiler/emit/bytes_runtime.rs` | `8981205e6322aeefbef77860a5fce12f087e97a96eff8847fbf574d72fee0910` |
| `src/compiler/emit/c/literals.rs` | `4452e56917390a0a5794d747719040c79aee3c6037882301814333f1140ec7da` |
| `src/compiler/emit/c/mod.rs` | `1152bbd874216a4c33b48092c1240ea679808b73933b7010d70b0a8376386906` |
| `src/compiler/emit/cause.rs` | `8b6ee7d725a47f4cafc72d3c37a2afe111ef688334b5ee759288d2351c874a16` |
| `src/compiler/emit/child.rs` | `944093eda00f6d3c24bce6cebb1d20e0707ec4cb9a6309cf41fc7deb95e9e982` |
| `src/compiler/emit/const_bridge.rs` | `34efb36d10a604d2235c01b83b0fd8e4091381066e44f4bef380be1e0bda24f1` |
| `src/compiler/emit/const_runtime.rs` | `1dec3504ea6127cdd9cc88c01e858378411e545f536be1298c149c7b3a77911a` |
| `src/compiler/emit/control.rs` | `76ad13adcdadb37de4610f5db824d8ee57411121e6e8dd8ff71a2461fc8a674b` |
| `src/compiler/emit/deferred.rs` | `94ac57ad288ea49ba1a2d364d8bec36fa7943d335ee9bed9a1ea8aae6b195802` |
| `src/compiler/emit/derive_runtime.rs` | `cbc7040169026ceabba749b7ec35801476285ce981a30d5a09a662aafa644b2a` |
| `src/compiler/emit/entry.rs` | `208e41ee75ea8a79fa23713ee54d2a5e6380f4971fc8678391c1eba7ed5fd087` |
| `src/compiler/emit/error_runtime.rs` | `b14e214a237c019409d97a722294b86841ca3f0e0727ac63b61833904ab709c2` |
| `src/compiler/emit/external.rs` | `a0121d28473fcffc5b3220c5099eb6d13349075b934729a76f16c323d48cf990` |
| `src/compiler/emit/features/builtin_utilities.rs` | `822486390d1c5c279e9b8df1339c4699011d81944f3dddc310eecb586f7e495c` |
| `src/compiler/emit/features/builtins/bytes.rs` | `0aed344ba98b326e51e7bde0521fb7b86fdf8f3472c712d81f9190396e9ee1a3` |
| `src/compiler/emit/features/builtins/cells.rs` | `b143402003dd2eadc847546ebec9b4e257ed60a1586a96b28e343a53ece4342e` |
| `src/compiler/emit/features/builtins/float.rs` | `1ac19f0027147b80d368f1a4d27fbe2ab75f3ab2bbf24641a6b1c4b9cfe254d9` |
| `src/compiler/emit/features/builtins/float_parse.rs` | `416c884f7386472220469ff71117ddb2126408716f44e03ffe3db0fa042700f0` |
| `src/compiler/emit/features/builtins/float_to_string.rs` | `c4678d54414c7b6a4946ad846882bd89ec81b2f5b8e4d43b6567487dd3cbd654` |
| `src/compiler/emit/features/builtins/int64_to_string.rs` | `e13a5e0c65a4ced59134959e5016fe1113d1fbed0b88b4c2bc0bb437dc17ba3c` |
| `src/compiler/emit/features/builtins/int_checked_add.rs` | `34a97b6617765479fb864e94e9d637ffc141a5554a8c8e3cc7a8dbdc386a039a` |
| `src/compiler/emit/features/builtins/int_parse.rs` | `0757fed4e1208f1d4decc92520b454d5ebd3fee0ac605139b526ecde8f310fbf` |
| `src/compiler/emit/features/builtins/int_to_string.rs` | `b1105fece2e137a416840c6db526498ad8e113ff505c9969ba92702dd0121c39` |
| `src/compiler/emit/features/builtins/integer.rs` | `81a0fd8bbb31fb3db59873de1161322573b8f27c0a6f4dfa36a8bf208f1d4c83` |
| `src/compiler/emit/features/builtins/integer64.rs` | `944b74699bbc5954894b88755873ce0786548bf7ebefda0b10ab39f265ca071b` |
| `src/compiler/emit/features/builtins/mod.rs` | `d1b3a197f8b45a6efc5c0b7675b1f34cba6f091f9b9ce40b69dd6dbaa5c6d779` |
| `src/compiler/emit/features/builtins/rejected.rs` | `fe3438dd128fb72b8b4b2385b3b347b4f3333fea0d752130fddc942c85c76f7a` |
| `src/compiler/emit/features/builtins/string_append.rs` | `6e975473e6bef48d436b4067cbac458395b9a032f72680df5a18a11c4c39e8d8` |
| `src/compiler/emit/features/builtins/string_byte_size.rs` | `706e7f54668d01af8056a992b4513ed1d3f61c603986531cad466d7df9d4992c` |
| `src/compiler/emit/features/builtins/string_chars.rs` | `436987b5d926bea1c737c3b5fec87a51a8a19c0ef838a918cddc3bee2ef73aca` |
| `src/compiler/emit/features/builtins/string_code_points.rs` | `e6ad45408a8764fda657edec0983bb24c17d2a942baa9ffb4f2712c04093aed3` |
| `src/compiler/emit/features/builtins/string_contains.rs` | `98afc37267d5c8d899ac16fa7e05484283c20e64eb4c362f58da0d964533b09f` |
| `src/compiler/emit/features/builtins/string_ends_with.rs` | `21c6249920667b9158f8abaf60a1748e8a6e4b8cccd2f51d6bdf1a8570131f2a` |
| `src/compiler/emit/features/builtins/string_from_code_point.rs` | `f78728b3c8ae03814483973080976da993537ad3ba550c8bddc40540788a2eaf` |
| `src/compiler/emit/features/builtins/string_join.rs` | `19bd30edfbfd96a2991e130c48f62cacee8457cbe2523e4e85bbf3d4da34a120` |
| `src/compiler/emit/features/builtins/string_length.rs` | `3f43e4b27641a9451d4b18e5856550451c5c081aa8583bb37394556f9b481e35` |
| `src/compiler/emit/features/builtins/string_split.rs` | `af8688447c37062fc5ea398eb960091103538aa51a347e6d65e70ada41cf89a2` |
| `src/compiler/emit/features/builtins/string_starts_with.rs` | `fec24b32b516e782977187480ed5a38576793c5fd05e857aeac2aacd77970d23` |
| `src/compiler/emit/features/builtins/string_trim.rs` | `2691a058ebdaa995d40b9e78328d773639c799376029f2869e92cf26594f6109` |
| `src/compiler/emit/features/case_keys.rs` | `5448c34a4972d2c76a2afdf77616824b0ab2063a647cc756a9f1f2ed0b189b5c` |
| `src/compiler/emit/features/constants.rs` | `410468318d06197ab536c20c6a3d593e5a3c4af3bafd71461956b1f288133159` |
| `src/compiler/emit/features/control_utilities.rs` | `96bd77ce3403a9d502d3585aefa047240322ee47631e5d2d2128ec1179c1d30c` |
| `src/compiler/emit/features/failures.rs` | `a87ed5678091947495d63bd6aa26f618f1893a95415a472cafcd8bc6f3ac7280` |
| `src/compiler/emit/features/fibers/attempt.rs` | `727bb0b538f0461ec764834746f8608476d4b6d8b828fc73b5af6a0665e85ee1` |
| `src/compiler/emit/features/fibers/await.rs` | `a79517aafa773c6e5a000c27a5e9377f5e49e87702a908cbe50049f24253bf3d` |
| `src/compiler/emit/features/fibers/cause_kind.rs` | `d0dda6ef78c40056f7507ba4b92c109b3fc1d61973a7c6782d4aa77f2aabd658` |
| `src/compiler/emit/features/fibers/cause_raise.rs` | `ba892d131cca16981d26a971a42d02373d9b65f8c54789515bd87354e42f563e` |
| `src/compiler/emit/features/fibers/cause_to_string.rs` | `6674bc35a922fb567c0d50d47ebf4730384c632a8834b123324fd08e20ee3639` |
| `src/compiler/emit/features/fibers/complete.rs` | `624caa48aaaf460654cb6779a3d00f4ce896d1d0270e2e6b837dd514d71b4c77` |
| `src/compiler/emit/features/fibers/deferred.rs` | `ef380e111267a41fc7172ce5f07557a7d8afe41f34c5be688a1d300ccc0e739b` |
| `src/compiler/emit/features/fibers/deferred_await.rs` | `36e87088675f83bad73ab48e2900c9873b01ca80e7c177741af59fedfb0fbc28` |
| `src/compiler/emit/features/fibers/fork.rs` | `ee75a131553bc685fa19382044a6572258bb5d47b52e706f5cf2f80fe124beaf` |
| `src/compiler/emit/features/fibers/interrupt.rs` | `4c8764ac34c3ee72049412088ade13727750cb189a1cd65f1d86b7c0a28c4d78` |
| `src/compiler/emit/features/fibers/join.rs` | `aded6ef2400350e02a205849e42ad824c65a78d9e5213f05a9914d3680fe72f9` |
| `src/compiler/emit/features/fibers/mod.rs` | `0e294981446bdb438fbafdec54ab9587ddf035b06f4c89fd8711d0ac8c27b084` |
| `src/compiler/emit/features/fibers/poll.rs` | `837fb5454e70eb3bc24fcc92dc740c97807be39a1ea05efafc4bc6bc4124d0e0` |
| `src/compiler/emit/features/fibers/scope.rs` | `6dcb793056021bc2991ccac93e00f4cfadbd53853245ad00a3dbf461ff103951` |
| `src/compiler/emit/features/mod.rs` | `6f3e8504ec2b89b4f3038c11e7f91d8abe9fb6fcbe7382c8f3b797319311ac1e` |
| `src/compiler/emit/features/operands.rs` | `5ec85350ee88d30e7366f81bf109869d1ec804f27e26288de825e8ae11c7e8f7` |
| `src/compiler/emit/features/primitive_operations.rs` | `4d49a28a732bd14b84af540113575cba52c5d2af224bea3d6b4f703303907f93` |
| `src/compiler/emit/features/primitive_types.rs` | `4e67fc09ddac1ba990d895c6e8138cfcfdc0c626eda7f9ddafe629023b8a9f8d` |
| `src/compiler/emit/features/runtime.rs` | `dbf87cc732e81e7c58c0262023506c6d86360000f1c3ad68bcd94985e6f177ee` |
| `src/compiler/emit/features/statements/block.rs` | `c4967777e84387137b6c907a5a0de53ac4b50b58d8bb3e60a3788b93cba35484` |
| `src/compiler/emit/features/statements/drop.rs` | `b577e49221a7d94a41f7d3d7a831cfa5bbd92b3a08c5023c764b25c5f6128a04` |
| `src/compiler/emit/features/statements/dup.rs` | `7f523a4e17234fe0df33760d1501959319165eca666b4344ce8586068854deb8` |
| `src/compiler/emit/features/statements/if_.rs` | `6464889458959d43aea6f1eb15c3a1f81ccb3f9c86dd11e61399b717294c1e00` |
| `src/compiler/emit/features/statements/let_.rs` | `ff44eefd335e5fcb760db72b4d4ce092d840a5bb40f5be90c03053314192b681` |
| `src/compiler/emit/features/statements/loop_.rs` | `40dde8e1c6c422079eae81cf58ce8ad535a065076012b833124ecf2c2cfa3c2a` |
| `src/compiler/emit/features/statements/mod.rs` | `c87ad99b1223840a8228e1728c69f408992c268001d6f61e30b97f69ef85b9ef` |
| `src/compiler/emit/features/statements/protect.rs` | `329b87471b127e462edb2d516eed5826154d2e2fbd2124abad8a9a15f1d5b23a` |
| `src/compiler/emit/features/statements/switch.rs` | `29740d3bea8e7d015b2efe56a3f3b6582629dc6b9a17ca64603e5434449ad2c2` |
| `src/compiler/emit/features/switches.rs` | `303568c09fac01248b47b8438a6f03244408caee932aeb03a685eb120b73d257` |
| `src/compiler/emit/features/terms/break.rs` | `581d73c970da79cdba18065339af58352e0885282d304158d714731c4684e163` |
| `src/compiler/emit/features/terms/defect.rs` | `931061596a5b174b2dfbe5d0010c4df3f70c9f421d018ba19f389efa3b7485e9` |
| `src/compiler/emit/features/terms/fail.rs` | `45fe5551e25f7d2d38acc47cc308b58b2ec4d48fb6a227c536f19e75466922f4` |
| `src/compiler/emit/features/terms/mod.rs` | `9874c3c375e69fa4b4a9c677afc366b2c5f570d42a1b6d8ec5de6275ee2e5b0a` |
| `src/compiler/emit/features/terms/recur.rs` | `d0c5ba3060e9a663fec0612273d4d6e97aa80bcf3c5b3d95eb5be26e8423e449` |
| `src/compiler/emit/features/terms/return_.rs` | `4b5d38f413e9d09a3ec585c224be21643119bf2ec02944cea950b279c5689778` |
| `src/compiler/emit/features/terms/unreachable.rs` | `a62d7045a1d174d27f915d783bb4341db4d9dcf2923e4fe2ae3f7e9a4a752eed` |
| `src/compiler/emit/features/terms/unwind.rs` | `c8d76596137286c83b357807eaf19496c0d3def18f23a90c7bce86cf50b9c881` |
| `src/compiler/emit/features/terms/yield.rs` | `7f645037a9c526d742f2906d987e1deb7c5a063bdc84718e6126a30ada93c1b4` |
| `src/compiler/emit/features/values/as_error.rs` | `754fd61f088ff44b7e18ae1cc3aff91a663bf0194efe42253b7300f1eda21dfe` |
| `src/compiler/emit/features/values/builtin.rs` | `e8295cdc705b851e4ed8b904d419540d87867efb1a8e9f9f57756c15d63db4d3` |
| `src/compiler/emit/features/values/call.rs` | `c91fe03a03818c8b28b8556f8adef8fb69a59a8204c1c2fcab4484dcd4a805a5` |
| `src/compiler/emit/features/values/closure.rs` | `8da09be58f9e28fc800fb7ee55c05ab6300fdbd4e8c5945468122fb4a654d91d` |
| `src/compiler/emit/features/values/coerce.rs` | `cee6471aac14955a28f753494d90da6a88d672cd06b693241b7d5535dcbf116b` |
| `src/compiler/emit/features/values/const_.rs` | `a13b73b75f030e9f1ef8412d0fb9438594c3633ddebb679c00729471db341f91` |
| `src/compiler/emit/features/values/env_get.rs` | `3d8bcf441c92481c8923346d2d255db42f58ecc4a0791ab9c7e83956f98a2759` |
| `src/compiler/emit/features/values/ev_extend.rs` | `c9a1dca6b2517f63c3c1b3ce812ea796a6cb81a16ee91a83432525f8b945ccf7` |
| `src/compiler/emit/features/values/ev_lookup.rs` | `d150211332b0613f4638af015cf6c6656d252cf4860e5e2f5648c0b8f366cb39` |
| `src/compiler/emit/features/values/fiber.rs` | `a920db1fa2340bf397b255c28aed064c5901e43a5980e3cfd0b6f2a8beefab17` |
| `src/compiler/emit/features/values/field.rs` | `674e947bd6489c5dec94e46953dab2e0ee6162daa94d2293527e681a8603198a` |
| `src/compiler/emit/features/values/fn_ref.rs` | `309287080001e408b6d3f8a24f41d954ba792b6877f806c89aab4af22ca87a01` |
| `src/compiler/emit/features/values/handler.rs` | `bb78e88201d1be1b4ba29fa0d3d97e5a461df34b8b244836269881c4eeb44af8` |
| `src/compiler/emit/features/values/is_error.rs` | `e3cd78603908464a1fbe7cbc01631a9b115a09ec12728c127db383c5be8a37a9` |
| `src/compiler/emit/features/values/list.rs` | `fd8713b73310dc3fe408c5be88569c3fa11bac7b34bd58415f4b6d9f2370aafe` |
| `src/compiler/emit/features/values/list_head.rs` | `89cc0d9950ad1f00e57169e9bb837ca9002714c9554789ad693725e80850e311` |
| `src/compiler/emit/features/values/list_is_empty.rs` | `438eb194fa9dd309ff97450f06933dc0658e93e0e1d2acef9bf79c9ea94ec03e` |
| `src/compiler/emit/features/values/list_tail.rs` | `e2111d5d8437fba28a85bd9f6fa05ce6e1cc1eb1ae97f26a2b3e37406b6862ba` |
| `src/compiler/emit/features/values/make.rs` | `396510ee908c952f1e7e3e8708210852ca0a8e55ea409a59ead4cd8e04cf6681` |
| `src/compiler/emit/features/values/make_closure.rs` | `5406e08a921988db8aaf4c4b23ccec4c8baf58339806992f01308740f68c49dd` |
| `src/compiler/emit/features/values/mod.rs` | `455536988829f0db6d4262cdebe028f3d95ad52e585c442274a16dc79adb32ca` |
| `src/compiler/emit/features/values/prim.rs` | `24d406499a3de7f1b987637fb7d70a7ed6ae54feb9481b9642e7bbcce0e417e4` |
| `src/compiler/emit/features/values/runtime.rs` | `aa6ea0c671fead0846cd8df8f71e6df5dc465c47684cd2022607aef520fc2a78` |
| `src/compiler/emit/features/values/tag.rs` | `234474320159ecf13e34b557e828fb3bbde832d4af159ca77fbc0177e0027ceb` |
| `src/compiler/emit/features/values/tuple.rs` | `e0eed6c6834b45db37089bf049a6c8bd92b2861e26ec3b4acf4c879a934851c6` |
| `src/compiler/emit/features/values/tuple_get.rs` | `e5e8b2d9ba2a7761d31093698b3c13078dedbfa331fbee1315734668e65c2802` |
| `src/compiler/emit/float_parse.rs` | `6e55a54fb8adf8df4fb4878797113601e60d836bbc8affd929cbcd132b45882c` |
| `src/compiler/emit/float_to_string.rs` | `48b11c2ef06c828166ea011150490e77096ac43c8616f82870a0d453ee864a5e` |
| `src/compiler/emit/functions.rs` | `8fd7914eb22ce8cbce0136770e3ec72812852cf157213fe5dd1128c7b21ed957` |
| `src/compiler/emit/header.rs` | `ee42bdd4b1efb6b9768aea8c151997ffcab20d829a2edbe12752ff4988762920` |
| `src/compiler/emit/heap.rs` | `e413884cc7f92cc681b25cf016dc92cb231108666e61d9a37b553ae02a7e8ecf` |
| `src/compiler/emit/layouts.rs` | `35c6fb43abd45d4ebe2d2502811d3a58b2a2b07f1c93fa4b589e41a6664848aa` |
| `src/compiler/emit/mod.rs` | `5b8c10dc9f3bb580f94824906a5be442102a01c11170f1580387bd2824e171fb` |
| `src/compiler/emit/numeric_runtime.rs` | `6f07a808ba9051afc6507880a10a592336645b5970bb5661452c8db785434503` |
| `src/compiler/emit/owned.rs` | `35a71ee57ed2b3aa973c457a8d8e8e9637d4138e31cc472aac0302aff5007867` |
| `src/compiler/emit/platform.rs` | `01ddbc93197782db9faa5f3c560fb55e069251f4025d4c07c7ce1e7529ae39db` |
| `src/compiler/emit/runtime/child_wait.c` | `fee0bd1bb820b9684955a2830694a5920c26efc1f269cf33aa09834160c0a3de` |
| `src/compiler/emit/runtime/const.c` | `056d4690be51f2dcc2daefd9b30f0cb9fa6b0269bffc151a8be6c6dc8919baba` |
| `src/compiler/emit/runtime/deferred.c` | `958045fda98fb9d088a0e541889a8cd3aceeb5e61bb54a38b6bb640f65c5f56f` |
| `src/compiler/emit/runtime/deferred_await.c` | `d5b829486e0d0eaea64bdedfa960e82aefce605bb31b7b4e8f2e0c29fde79fa6` |
| `src/compiler/emit/runtime/deferred_types.c` | `f012d42ceae2ca7903bb3f58980cc762ab5ed3aafe255e33f575e281550ad662` |
| `src/compiler/emit/runtime/resume.c` | `abf32f8f8bad3075d68a1286e1d8291c8dd5866d013a5e50a0154371502406a2` |
| `src/compiler/emit/runtime/scheduler.c` | `c18f90642e12e6fc001be2ee2b4613e2dceac85157b9cb0b16ad8ec2779bda4e` |
| `src/compiler/emit/runtime/scheduler.h` | `358811abf04afe98ff2e4dfeb799ac7183458f2b27d61ced6b749abab0b9957f` |
| `src/compiler/emit/runtime/scope.c` | `f96dba81754feb3a8783e5de2b8209f1f35e98a6ff1eecc927a67a69fa87c8ba` |
| `src/compiler/emit/runtime/scope_types.c` | `5f46c620298d0df516a151d218fa14c9a6ff68f0833e6c1e28d526d1341a70f7` |
| `src/compiler/emit/runtime/task.c` | `84047a327aa9aa40e928275436a096dd76fecb8e95367f0dcbfbf020727ddca7` |
| `src/compiler/emit/runtime/task.h` | `5a77c1fcc17fce82eadda1d8d73a20a888936d9489b066df6d2229960377f247` |
| `src/compiler/emit/runtime/task_context.h` | `89464fb859c0c550ac7cd22ae930b272ac9d6b1851bda39ecd5e859ee344285f` |
| `src/compiler/emit/runtime/task_failure.h` | `7c70e19f94ba445e027bc5737282be4d348aa24706d0c72c8ba87843e49d4e5e` |
| `src/compiler/emit/runtime/unwind.c` | `b22dc94b8efa871d736ec5250e32c6b7bf62474f5501c39abe7734d9a1bea1ab` |
| `src/compiler/emit/runtime_helpers.rs` | `cc109c37df79e96eb13f13a06894676075933ea7d65b9e63afbbe9a06bfc6a12` |
| `src/compiler/emit/services.rs` | `cedc18d3b714a40402ca413dd41992abfe054602c59a32942dd9c4ae6989f711` |
| `src/compiler/emit/string_buffers.rs` | `7018927607be8f9859a395f98ee4244f6a31baaf7119185b5896d8529c31316c` |
| `src/compiler/emit/string_runtime.rs` | `131f695f88ef408e52b02a4690afe8abe26a33b61d191b4ecd4276ba0f9163c6` |
| `src/compiler/emit/suspend.rs` | `9c1d22fb2b8a615f845eea5f0c92072ec26b0c6abb0f3a128a8b0811ef4ebe55` |
| `src/compiler/emit/task_values.rs` | `17eb3ad31b5ec3279d0787b598425780ba340a98f56be83fea05fcc53e4fac5f` |
| `src/compiler/emit/unicode_grapheme.rs` | `801d8a5c1f2668d9a22dd983c58bc78fbdc687e4b1a318c05139c7ff0233e0f6` |
| `src/compiler/emit/values.rs` | `2602784c7a73f90f2ad41082709c2300ddd82dcde380214a13a2fb621f18ad2c` |
| `src/compiler/plan/adapters.rs` | `6a93e3acf0b44818f9cb68dde61e10e0b567f617dfb8a9d8b17dec896787b0c2` |
| `src/compiler/plan/analysis.rs` | `3c6d9d57d15d7a1c1df9f1a5136f2f4eb45df11a7287bdad4a9a918aedbbb86c` |
| `src/compiler/plan/const_bridge.rs` | `38e2f312a017215833c2183d2ec3b9e7aebb7a0caa7a31e5f73ee4048d44dae7` |
| `src/compiler/plan/const_runtime.rs` | `fa77b5dd50a4bee7e021443b7d696fccd6afe7cdf36f1430bea1317630a416b2` |
| `src/compiler/plan/derive_runtime.rs` | `471bd890567663ac68ee0b76f96fb37e8c8b5266afe6b228bc340a21bea3c4f3` |
| `src/compiler/plan/external.rs` | `5aae496764952b62586d13509f994271a534045d95569ccd191a001176ca0fce` |
| `src/compiler/plan/heap.rs` | `2647d9c1a0ec173f78b9937fb00be1e3a386de87bf5008ce27684a1358ed9768` |
| `src/compiler/plan/interrupts.rs` | `ebc49079b15b82413f6a42de3f38d7243274f5a384ef0834fc8382e665691488` |
| `src/compiler/plan/layouts.rs` | `4ce06f12d1fe2bad4ee8564c8eab3532a8fb78e883153885e4aa8bd345a153ac` |
| `src/compiler/plan/mod.rs` | `c0945499de6ad22cc3bf6c8459b3dbab7d793ab5ca0f15db592e818d3921826f` |
| `src/compiler/plan/operations/builtins/bytes.rs` | `1f89c5d3e2c6e07be0665017f23582c1aec21e929f4bc2bdb886ddd8cbfd553f` |
| `src/compiler/plan/operations/builtins/cells.rs` | `7e7a280434e874b0e92c9ae22db7de03e073fff8cf6f9ce4dbfd7362b6524197` |
| `src/compiler/plan/operations/builtins/float.rs` | `f7033e82b9f11a3f67c8d2a8947a98e9aaebd5b14079ec269d21e9229248444d` |
| `src/compiler/plan/operations/builtins/float_parse.rs` | `d8dc57b4a2c7df33217bb427fc3df3a9bb98fe66faa93c2cd9180036697db7bd` |
| `src/compiler/plan/operations/builtins/float_to_string.rs` | `7a1fc624b413f3b28e51056ecb13f22f109c385af97f3f3be32e9f620023a15e` |
| `src/compiler/plan/operations/builtins/int64_to_string.rs` | `c0ee8555a7ef2fb8ed057d33c9f95262ec03a75eb378741ad36e0d42d90dfc85` |
| `src/compiler/plan/operations/builtins/int_parse.rs` | `9483873e2919b4b45c823fc432170dbf43ebdbb6e5e0562dd58388eb8a7e7844` |
| `src/compiler/plan/operations/builtins/int_to_string.rs` | `e1e099cff998c0a10817a43312d5de5f4be9d6779f20b9abda6449d353dcdca5` |
| `src/compiler/plan/operations/builtins/integer.rs` | `da015985797b65235c1f7c529ddf78dd1916fc8f0fd0627697cc56e32d262bac` |
| `src/compiler/plan/operations/builtins/integer64.rs` | `305a2df1b58672153b4d99f67215df52555f23716d53e160f3434c582c1db8a3` |
| `src/compiler/plan/operations/builtins/mod.rs` | `5afa9f072935e09b695f48c7b7651bb4f245f9d5fad3602f37d78815e94d4e88` |
| `src/compiler/plan/operations/builtins/string_append.rs` | `da69653c3713ff8a926080c431ac5a91b9b2b240c1440db45423a1de0c3b5eb6` |
| `src/compiler/plan/operations/builtins/string_byte_size.rs` | `7d827473e21b77cfd0d224792e6f62b3d4c5617f1068d4c492070897b94834b7` |
| `src/compiler/plan/operations/builtins/string_chars.rs` | `a072412f8fcd0e1873105101d332a420ae79b55e0582c436ad01057ed65a7ede` |
| `src/compiler/plan/operations/builtins/string_code_points.rs` | `ac650e1162689d71ace861d2c6585f39602740904d89234ed7d0bc48f6f12a2d` |
| `src/compiler/plan/operations/builtins/string_contains.rs` | `01afd7ae35cc02eb054eb4ad90388857cae35f4444cdb6788b9628836bed12a7` |
| `src/compiler/plan/operations/builtins/string_ends_with.rs` | `60ce2ee81ea4f3dfbf5dcafa12c2410cb0f797a1fc5525521abb02887cd4f0f4` |
| `src/compiler/plan/operations/builtins/string_from_code_point.rs` | `d83432d6f00ad9c037d716b77375e332f357eb49a45dea3ec8dde73979d5b2aa` |
| `src/compiler/plan/operations/builtins/string_join.rs` | `2f81f4e7bc12f1eebee7d5cfdd9c932e38ed1c3a4af96bfdc9ed5f755ea623cd` |
| `src/compiler/plan/operations/builtins/string_length.rs` | `5290ae35e18b8df81dea1e78717e339821a359ba8f350e44a44428dedd4d1bbd` |
| `src/compiler/plan/operations/builtins/string_split.rs` | `1a2d06052740f6eec7cc9254d56999a4dec7244529b15eb38801a21ef35a58ea` |
| `src/compiler/plan/operations/builtins/string_starts_with.rs` | `2ddae8912e5e006df9ae83d494a47c28daf52f6efd67c520e65ac86591af6037` |
| `src/compiler/plan/operations/builtins/string_trim.rs` | `9d363fc6b2041b93494416950a99776f7ac3d39b4498637b31d941cb57ce9f32` |
| `src/compiler/plan/operations/failures.rs` | `e07cda26bed3268058ce01510a6e0c99f1f5d7bd15b93a14feb05107764700a9` |
| `src/compiler/plan/operations/fibers/cause_to_string.rs` | `f6ef0e14b37a058ff8a71b4e09259859d101f87e0e9ed014bc8bbf36bbed4ecf` |
| `src/compiler/plan/operations/fibers/mod.rs` | `521339249326c2558b8b55e9a0ba636966d7ac234c2e713f67707d59df1d6860` |
| `src/compiler/plan/operations/mod.rs` | `49dd5d0ad1423f5eae880f0ea85084a109dc7d5e73ce1d90c01009493b08c937` |
| `src/compiler/plan/operations/primitive_operations.rs` | `a35891a23bdf8eef11a49b8de1028d5db15caac1386a8183564963a3a274471d` |
| `src/compiler/plan/operations/runtime.rs` | `0f69738c7f2c3a73aa571a4f2ce55188449c90f080f69a787f7c9eca107ad704` |
| `src/compiler/plan/operations/statements/block.rs` | `61515f07f968d8b3dd581571ef400621e42c7db681b14b41328b6f3afbb28eb1` |
| `src/compiler/plan/operations/statements/drop.rs` | `ff260d904ae81980f7975593c196863a194f0ea40e251c8ce45ee92647ed63d2` |
| `src/compiler/plan/operations/statements/dup.rs` | `ff260d904ae81980f7975593c196863a194f0ea40e251c8ce45ee92647ed63d2` |
| `src/compiler/plan/operations/statements/if_.rs` | `29a1b0f51832303ba40acad8bdb4e7e30f03cc6ede424208faf55c6483f02408` |
| `src/compiler/plan/operations/statements/let_.rs` | `09dde1e5e60fdc67eb2c2ddfcf28050efe3ca05009b93cdb8ca210320f2c9157` |
| `src/compiler/plan/operations/statements/loop_.rs` | `6c8b121f748d6642e1886c7183a431b49126c6d5097ee57853797be15da3ca6a` |
| `src/compiler/plan/operations/statements/mod.rs` | `b58c7e7b7a3a8ddc5441ded72eb69a4468fe40676e0eb9eda48e5d6fa8548636` |
| `src/compiler/plan/operations/statements/protect.rs` | `f14359f0e06e6d9bfda0d35c7b98e6babb9ff4eec49ea88f0be575d2716d293c` |
| `src/compiler/plan/operations/statements/switch.rs` | `609b6ea9c61bb9c62c5fb835cacd6cc46629a8250a46501f655595a7c31cf675` |
| `src/compiler/plan/operations/terms/break.rs` | `bf0004a9d60148e233306a3f7505fff5e353a681325419aa9e5547c011727a3e` |
| `src/compiler/plan/operations/terms/defect.rs` | `f72357a04631cb265743b61c18ebcf09b328b80fba15ce32a20377568351fa25` |
| `src/compiler/plan/operations/terms/fail.rs` | `aefc797b0413baec182467c450768937f554ce9da71e4a7653e19d2c3eff648c` |
| `src/compiler/plan/operations/terms/mod.rs` | `9abbc7eaea12799eabd488d0ba57ab8bce2efec1a5c0c1a55508a123add47114` |
| `src/compiler/plan/operations/terms/recur.rs` | `f3dddd95178cdd635d7ec176cd59dc8e88674b816480e9cbc998249e95b12661` |
| `src/compiler/plan/operations/terms/return_.rs` | `a418fa2bbc5a4d5ddbcdd1f641b33bd4c406d088ced12ca244b2e18ed5edf8f9` |
| `src/compiler/plan/operations/terms/unreachable.rs` | `85019760eb747addcd4a17dbf24cdf242f484d36253c99ac14643d49cbdee3ce` |
| `src/compiler/plan/operations/terms/unwind.rs` | `59620a507ad19ffc5bf6f677ab53bbfadfd5d1a9d75b30d266f1c45b028d85d5` |
| `src/compiler/plan/operations/terms/yield.rs` | `b7fb0bf7f8963ff96c2c834ce61b243a74034c45824035b9d793bcc0b962cac0` |
| `src/compiler/plan/operations/values/as_error.rs` | `3225cdb388d4f45ac5196b6513f5d31e99b51f59a4c0892e5ba787230baade4e` |
| `src/compiler/plan/operations/values/call.rs` | `12e3bfaa718358d13d5ec2875f6de952f47d86c76b27293af6c8a73d5d501ab7` |
| `src/compiler/plan/operations/values/coerce.rs` | `5563402d285e935f15e6a36607decba433eb5a603f2dad9187b45642056301e9` |
| `src/compiler/plan/operations/values/const_.rs` | `6775941b813663f60e82757b3d278ed3436ed53ab48c92c261ff141a932ca94f` |
| `src/compiler/plan/operations/values/ev_extend.rs` | `28072cad2c7073e0853b189c7425d8cf68588a07003a0b172e720e4f8193c6e8` |
| `src/compiler/plan/operations/values/ev_lookup.rs` | `8661ad0f3a882f0b3f12fd70861674b6b6cdd9b90eba2a5be39c3634a5ebf4f5` |
| `src/compiler/plan/operations/values/fiber.rs` | `84017f3f9362d17003a8693fdae5cfc99f829f318cada14b80fe02c076cb2849` |
| `src/compiler/plan/operations/values/field.rs` | `3225cdb388d4f45ac5196b6513f5d31e99b51f59a4c0892e5ba787230baade4e` |
| `src/compiler/plan/operations/values/fn_ref.rs` | `3bf49e09b5bc16452c67e52e7a93b382b2ad77a17f1078925b559b1470010720` |
| `src/compiler/plan/operations/values/handler.rs` | `a604662a311994e5575209faf71b16a1a2c82116b61e22c696511fc927a43053` |
| `src/compiler/plan/operations/values/list.rs` | `32246536b74039867c8edfb6def9e604bd16eb4a6d8ee3b9f3b3587db54ee5f5` |
| `src/compiler/plan/operations/values/list_head.rs` | `8661ad0f3a882f0b3f12fd70861674b6b6cdd9b90eba2a5be39c3634a5ebf4f5` |
| `src/compiler/plan/operations/values/list_tail.rs` | `8661ad0f3a882f0b3f12fd70861674b6b6cdd9b90eba2a5be39c3634a5ebf4f5` |
| `src/compiler/plan/operations/values/mod.rs` | `cb76fe00c230b31e2b6335ce3675a761dff856af4e571be67fcc49de51646b36` |
| `src/compiler/plan/operations/values/tuple_get.rs` | `3225cdb388d4f45ac5196b6513f5d31e99b51f59a4c0892e5ba787230baade4e` |
| `src/compiler/plan/runtime.rs` | `58d0303ba8b0cc05974dbcfa5c687ab9394e87399d1825ea9d61e575db124f44` |
| `src/compiler/plan/services.rs` | `e04669b8e3d57f9788d78851516fc99b8fb52ef8c3a3e9b843a5f5d36a7de150` |
| `src/compiler/plan/strings.rs` | `fd1857ded23a801a01134bd235c825768576d81f9a7a8e812835e75a6c92e293` |
| `src/compiler/plan/suspend.rs` | `9a0532f972ecfcaff130e80b7a6a1839108fbddb0e282fd5e0e3e2e48483278d` |
| `src/compiler/plan/task_values.rs` | `48eceae5971214ac349410979906ae7f57c2974c326eea887e498176849401a3` |
| `src/compiler/plan/validation.rs` | `fce9b0c199cdf838d9bbc5e7c42a36079555a38f196d7db7fe1e884d8ce57b53` |
| `src/contract/abi.rs` | `e30ab4ef7ce90b2d71027526e417ec65f1f81769f94afb08270bd13a93ad95d5` |
| `src/contract/artifact.rs` | `711cebdda7620604bdd36fd668569f8d20a1fe829224f8ebc11e65604cdcf9d4` |
| `src/contract/auxiliary.rs` | `6914487fa6281f34837b583195259015642ea8f0a244afb85d411986aa70cadd` |
| `src/contract/bindings.rs` | `f32c3d5642354a2318464edc53a007ea700f32da8706941b5fc1993373143677` |
| `src/contract/control.rs` | `9ea042945770492b88352482e60d076e4ee8e33145c2150407a8c0b74e6e1750` |
| `src/contract/derived.rs` | `b82054c5ce8eadb5eed89b0adf619e1991df1e8ac1d1c7b56b1edc1270177c2e` |
| `src/contract/error.rs` | `f333e180f8960af63f32b873a92d07490383724bfb65c0040426af0872a59394` |
| `src/contract/fixtures.rs` | `a91a8d4117741fc9a62184d5e7974839bbf0814da58451d0f04564b3129a079b` |
| `src/contract/ir.rs` | `3259e9a2641877f25d36bd3fedda85613f1c57c3912706948d193bc48de51884` |
| `src/contract/lowered.rs` | `02371b5fa36a4073aeff52671170586bd038d68eef55c0f6ec550456b12afe24` |
| `src/contract/mod.rs` | `7c23421a02fe4155563104e361168b2fef4d750ff0c35079aceee7ecd0114db6` |
| `src/contract/operations.rs` | `7ba1128d101b6337831e21d1c526d5680e305655740960068e77ed3ca37ed574` |
| `src/contract/options.rs` | `e4fe3f4fb358be4437c100394b14d99caa2d1a7ea8cac46e0120e7703ef5565b` |
| `src/contract/output.rs` | `53aaf05f12fb5253c991234bb089d5737817564b7bc2e5c85bf68d8b02a3e602` |
| `src/contract/planned.rs` | `45fc17aa084f070c8a6ab6e3591ce06abb404fa1d4ab75a279d82cc9e5804935` |
| `src/contract/runtime.rs` | `5facd41eb5fd75e83be701e5fba0ee1783af0e8aa7f55d099bcb6a931dfde7ce` |
| `src/contract/services.rs` | `1ad614f5fd7c77f22943f5f1045cf3f1c20f4b01a7ac0da3f48c82792cf3235d` |
| `src/contract/strings.rs` | `568832762490204546358cc04123ed674090b96e512a2f64751f809a04b5addf` |
| `src/semantics/abi.rs` | `5956a921a86d87a73b79b2e92bb6a38fec504bbac17d56a1dcad5e445b993fdf` |
| `src/semantics/c.rs` | `3340432d5318963b8423f960f559d75a13d38be20f701f5fb1194504c560f679` |
| `src/semantics/child.rs` | `1a70e894e365267aaa9f985f033b5131dcb5235730ee6a13975f5739fbf0ab32` |
| `src/semantics/deferred.rs` | `22cc0d2865db220417af8be3ee55199057e15a0b8858e02eb11169ea628f1a0c` |
| `src/semantics/derived.rs` | `f12fa1507479cc1efbf97e215e6d2e72bcc42c5d9bdee76538feefac69bf76b4` |
| `src/semantics/diagnostics.rs` | `dac2465f24ac2530f80d0fadefeed7cb605995430abb7c3caff693eb5c40a848` |
| `src/semantics/host_modules.rs` | `268e647581f0876e57d9261ce563358f1df613a0b40fdb77a950a1042a2da2f1` |
| `src/semantics/host_refs.rs` | `12e655132d20e870ede6db87951a0c146b27dcf92cd272403113167ed236ca62` |
| `src/semantics/mod.rs` | `b9548cad4cee7dd71133704d06f8f568dfba5c17f626c387ed698d5ff16b7aa6` |
| `src/semantics/numeric.rs` | `27dcb2112a724718d62d0e18925cbd01147fec8cce2b3c7465dc7ccff4b309f7` |
| `src/semantics/operands.rs` | `21d74d22c6266fbf879087c93854a4e1d7697e5339f68fd34ae651b798e5c35c` |
| `src/semantics/representations.rs` | `62779b714047ba2a8fa9ee347b56d60ffd4a6d7a7dc0c1b1b6235a7f86755884` |
| `src/semantics/strings.rs` | `a65b2fe535f1cae4b7584fc58f50303ee960738f15c13205f6f404ed0264929d` |
| `src/semantics/type_representations.rs` | `ab9ee2147ceb1caf6ad29e34bb28129829fb2cf136984d7c362c38595c9754d0` |
| `src/semantics/types.rs` | `2fb08b83ae8f9dd8593b78e18742969c4c4f58a2605ac584eb240de991bceb60` |
| `src/semantics/unicode_grapheme.rs` | `edf3a7fe2837451b7e13ca327d50f7b23bd442ca5770b96fb468cf57bd9f628e` |
| `src/semantics/variants.rs` | `7cae473c9ac8017efe10a862c3f5e67bbf57c4c16f0974ea3aa91884ad20ecfd` |
| `src/writer.rs` | `a462d3e365191cb862c608c5ffbd1a50f344688e7fbf3e0f14e37d2e92e47f55` |
| `tests/data/GraphemeBreakTest-18.0.0.txt` | `42045da24db676334a73916c203e0645107170466c658f476b40aac0c261a379` |

## P2の追加採用

`src/input/upstream/const_eval.rs`を`backend/lower/const_eval.rs`へ移し、入力を所有backend IRへ付け替えた。上流adapterやcrateは採用しない。4096 step/64 call深さの静的評価制限を維持し、静的評価できない単相constはこのrepoのlowerがruntime lazy cacheへ切り替える。元ファイルのSHA-256は `43ca4f814e79f2babb621e2778fb403f362bd28f9834adaba6485cb6e598ee33`（同一基準revision）。数値・Unicodeの意味関数と公式vectorの出自は上表と同じ。
