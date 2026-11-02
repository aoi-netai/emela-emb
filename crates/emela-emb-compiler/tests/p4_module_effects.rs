use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn selective_effect_handler_and_layer_imports_respect_public_visibility() {
    for (declaration, name) in [
        ("effect Read { fn read()->Int }", "Read"),
        ("handler Reader: Read { fn read(){42} }", "Reader"),
        ("layer Logging { Reader }", "Logging"),
    ] {
        for public in [true, false] {
            let prefix = match name {
                "Read" => "",
                "Reader" => "effect Read {fn read()->Int}\n",
                _ => "effect Read {fn read()->Int}\nhandler Reader: Read {fn read(){42}}\n",
            };
            let provider = format!("{prefix}{}{declaration}", if public { "pub " } else { "" });
            let main = format!("import Provider.{{{name}}}\nfn main()->Int{{42}}");
            let result = compile_sources(
                &[
                    Source::new("main.emel", &main),
                    Source::new("provider.emel", &provider),
                ],
                "main.emel",
                &Options::default().with_heap_bytes(8192),
            );
            if public {
                assert!(result.is_ok(), "public {name}: {result:#?}");
            } else {
                let errors = result.unwrap_err();
                assert!(
                    errors.iter().any(|error| error.message.contains("private")),
                    "private {name}: {errors:#?}"
                );
            }
        }
    }
}
