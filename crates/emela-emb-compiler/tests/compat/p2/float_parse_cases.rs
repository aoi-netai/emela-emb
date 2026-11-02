// Decimal vectors/generator adopted from crates/emela-codegen-emb/tests/emit.rs,
// dynamic_float_formatting_uses_a_bounded_runtime_and_float_parse_uses_the_bounded_parser.
// Fixed revision c34c0aeefdd966d854371b3110db00a47798751d; Apache-2.0, see repository LICENSE.
// No old frontend or IR dependency is used.
pub fn cases() -> Vec<(String, Option<f64>)> {
    let midpoint = "1.00000000000000011102230246251565404236316680908203125";
    let mut tie_with_long_zero_tail = midpoint.to_owned();
    tie_with_long_zero_tail.push_str(&"0".repeat(1200));
    let mut above_tie_with_long_tail = tie_with_long_zero_tail.clone();
    above_tie_with_long_tail.push('1');
    let mut cases = vec![
        ("+12.5e-1".to_owned(), Some(1.25_f64)),
        ("NaN".to_owned(), Some(f64::NAN)),
        ("Infinity".to_owned(), Some(f64::INFINITY)),
        ("+Infinity".to_owned(), Some(f64::INFINITY)),
        ("-Infinity".to_owned(), Some(f64::NEG_INFINITY)),
        ("-0".to_owned(), Some(-0.0_f64)),
        ("1e999".to_owned(), Some(f64::INFINITY)),
        ("-1e999".to_owned(), Some(f64::NEG_INFINITY)),
        ("1e-999".to_owned(), Some(0.0_f64)),
        ("-1e-999".to_owned(), Some(-0.0_f64)),
        (
            "1.00000000000000011102230246251565404236316680908203125".to_owned(),
            Some(1.0_f64),
        ),
        (
            "1.00000000000000011102230246251565404236316680908203126".to_owned(),
            Some(f64::from_bits(1.0_f64.to_bits() + 1)),
        ),
        (
            "2.2250738585072014e-308".to_owned(),
            Some(2.2250738585072014e-308_f64),
        ),
        (
            "4.9406564584124654e-324".to_owned(),
            Some(f64::from_bits(1)),
        ),
        ("2.4703282292062327e-324".to_owned(), Some(0.0_f64)),
        (
            "2.4703282292062328e-324".to_owned(),
            Some(f64::from_bits(1)),
        ),
        ("1.7976931348623157e308".to_owned(), Some(f64::MAX)),
        ("1.7976931348623159e308".to_owned(), Some(f64::INFINITY)),
        (tie_with_long_zero_tail, Some(1.0_f64)),
        (
            above_tie_with_long_tail,
            Some(f64::from_bits(1.0_f64.to_bits() + 1)),
        ),
        (".5".to_owned(), None),
        ("1.".to_owned(), None),
        ("1e".to_owned(), None),
        ("1e+".to_owned(), None),
        ("+".to_owned(), None),
        ("+NaN".to_owned(), None),
        ("NaN ".to_owned(), None),
        ("Infinityx".to_owned(), None),
        (" 1".to_owned(), None),
        ("".to_owned(), None),
        ("1\0x".to_owned(), None),
    ];
    cases.extend((0..128).map(|index| {
        let significand = (index as u64)
            .wrapping_mul(7_919_287_331)
            .wrapping_add(1_234_567);
        let exponent = (index % 641) - 340;
        let text = format!(
            "{}{}e{exponent}",
            if index % 2 == 0 { "-" } else { "" },
            significand
        );
        let expected = text.parse::<f64>().expect("Rust parses generated decimal");
        (text, Some(expected))
    }));
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    for index in 0..16 {
        let digit_count = 801 + index * 37;
        let mut digits = String::with_capacity(digit_count);
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        digits.push(char::from(b'1' + (state % 9) as u8));
        for _ in 1..digit_count {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            digits.push(char::from(b'0' + (state >> 32) as u8 % 10));
        }
        let decimal_order = (index as i32 * 41) - 300;
        let exponent = decimal_order - digit_count as i32 + 1;
        let text = format!("{digits}e{exponent}");
        let expected = text.parse::<f64>().expect("Rust parses long decimal");
        cases.push((text, Some(expected)));
    }

    cases
}
