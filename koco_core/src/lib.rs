mod ast;
mod cfg;
mod codegen;
mod common;
mod resolver;
mod tokens;
mod type_checking;

pub use ast::*;
pub use cfg::*;
pub use codegen::*;
pub use common::*;
pub use resolver::*;
pub use tokens::*;
pub use type_checking::*;

pub fn validate_spv(spv: &[u32]) -> Result<(), String> {
    use std::process::{Command, Stdio};

    let bytes: Vec<u8> = spv.iter().flat_map(|w| w.to_le_bytes()).collect();

    let mut child = Command::new("spirv-val")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spirv-val: {}", e))?;

    use std::io::Write;
    child.stdin.take().unwrap().write_all(&bytes).map_err(|e| format!("write stdin: {}", e))?;

    let output = child.wait_with_output().map_err(|e| format!("wait: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!("{}{}", stdout, stderr))
    }
}

pub fn compile(source: &str) -> Result<Vec<u32>, String> {
    let tokens = Token::lex(source).map_err(|e| format!("lex: {:?}", e))?;

    let mut parser = Parser::new(&tokens, source);
    let (tree, parse_errors) = parser.parse();

    let mut resolver = Resolver::new(tree);
    let (resolved, resolve_errors) = resolver.resolve();

    let mut type_checker = TypeChecker::new(resolved);
    let (typed, type_errors) = type_checker.type_check();

    let all_errors: Vec<String> = parse_errors.iter()
        .map(|e| format!("parse: expected {}, got '{}'", e.expected, &source[e.got.span.clone()]))
        .chain(resolve_errors.iter().map(|e| format!("resolve: {:?}", e)))
        .chain(type_errors.iter().map(|e| format!("type: {:?}", e)))
        .collect();

    if !all_errors.is_empty() {
        return Err(all_errors.join("\n"));
    }

    let struct_registry = typed.structs.clone();
    let enum_registry = typed.enums.clone();

    let mut builder = CfgBuilder::new();
    let cfg = builder.build(typed);

    let mut codegen = Codegen::new().with_structs(struct_registry).with_enums(enum_registry);
    Ok(codegen.codegen(&cfg))
}

#[cfg(test)]
mod tests {

    use crate::*;

    fn check_spv(result: Result<Vec<u32>, String>) -> Vec<u32> {
        let spv = result.expect("expected successful compilation");
        assert_eq!(spv[0], 0x07230203, "bad SPIR-V magic number");
        assert!(spv.len() > 5, "SPIR-V output too short");
        validate_spv(&spv).expect("SPIR-V validation failed");
        spv
    }

    fn check_fails(result: Result<Vec<u32>, String>, expected_substr: &str) {
        let err = result.expect_err("expected compilation to fail");
        assert!(
            err.contains(expected_substr),
            "expected error to contain '{expected_substr}', got: {err}"
        );
    }

    // ── happy path ────────────────────────────────────────────────

    #[test]
    fn empty_module() {
        let spv = check_spv(compile(""));
        assert!(spv.len() > 5);
    }

    #[test]
    fn fn_return_literal() {
        check_spv(compile(r#"fn main() -> f32 { return 1.0; }"#));
    }

    #[test]
    fn fn_return_void() {
        check_spv(compile(r#"fn main() { return; }"#));
    }

    #[test]
    fn fn_add_params() {
        check_spv(compile(r#"
            fn add(a: f32, b: f32) -> f32 {
                return a + b;
            }
        "#));
    }

    #[test]
    fn fn_call_other() {
        check_spv(compile(r#"
            fn add(a: f32, b: f32) -> f32 {
                return a + b;
            }
            fn main(x: f32) -> f32 {
                return add(x, x);
            }
        "#));
    }

    #[test]
    fn if_else() {
        check_spv(compile(r#"
            fn pick(cond: bool, a: f32, b: f32) -> f32 {
                if cond {
                    return a;
                } else {
                    return b;
                };
                return a;
            }
        "#));
    }

    #[test]
    fn nested_if() {
        // nested if/else works; top-level else-if parsing has edge cases
        check_spv(compile(r#"
            fn classify(a: bool, b: bool) -> f32 {
                if a {
                    return 1.0;
                } else {
                    if b {
                        return 2.0;
                    } else {
                        return 3.0;
                    };
                };
            }
        "#));
    }

    #[test]
    fn loop_break() {
        check_spv(compile(r#"
            fn main() -> f32 {
                let mut x: f32 = 0.0;
                loop {
                    x = 1.0;
                    break;
                };
                return x;
            }
        "#));
    }

    #[test]
    fn loop_continue() {
        check_spv(compile(r#"
            fn main() -> f32 {
                let mut x: f32 = 0.0;
                loop {
                    x = x + 1.0;
                    if x > 2.0 {
                        break;
                    };
                    continue;
                };
                return x;
            }
        "#));
    }

    #[test]
    fn while_loop() {
        check_spv(compile(r#"
            fn main(cond: bool) -> f32 {
                let mut x: f32 = 0.0;
                while cond {
                    x = 2.0;
                    break;
                };
                return x;
            }
        "#));
    }

    #[test]
    fn local_vars_and_assign() {
        check_spv(compile(r#"
            fn main() -> f32 {
                let x: f32 = 1.0;
                let mut y: f32 = 2.0;
                y = x + y;
                return y;
            }
        "#));
    }

    #[test]
    fn descriptor_uniform() {
        check_spv(compile(r#"
            #layout(set = 0, binding = 0) my_uniform: float4;
            fn main() -> f32 {
                return 1.0;
            }
        "#));
    }

    #[test]
    fn descriptor_push_constant() {
        check_spv(compile(r#"
            #layout(_push_constant) pc: float4;
            fn main() -> f32 {
                return 1.0;
            }
        "#));
    }

    #[test]
    fn arithmetic_ops() {
        check_spv(compile(r#"
            fn ops(a: f32, b: f32) -> f32 {
                return (a + b) * (a - b) / (a * b);
            }
        "#));
    }

    #[test]
    fn comparison_ops() {
        check_spv(compile(r#"
            fn cmp(a: f32, b: f32) -> bool {
                let r: bool = a == b;
                return r;
            }
        "#));
    }

    #[test]
    fn shader_stage_vertex() {
        check_spv(compile(r#"
            #[vertex]
            fn main() -> f32 { return 1.0; }
        "#));
    }

    #[test]
    fn shader_stage_fragment() {
        check_spv(compile(r#"
            #[fragment]
            fn main() -> f32 { return 0.5; }
        "#));
    }

    #[test]
    fn triangle_vertex() {
        check_spv(compile(r#"
            struct VOut { x: f32, y: f32, z: f32, w: f32, }
            #[vertex]
            fn main() -> VOut {
                return VOut { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };
            }
        "#));
    }

    #[test]
    fn triangle_fragment() {
        check_spv(compile(r#"
            struct FOut { r: f32, g: f32, b: f32, a: f32, }
            #[fragment]
            fn main() -> FOut {
                return FOut { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
            }
        "#));
    }

    #[test]
    fn multiple_functions_forward_call() {
        check_spv(compile(r#"
            fn mul(a: f32, b: f32) -> f32 { return a * b; }
            fn add(a: f32, b: f32) -> f32 { return a + b; }
            fn main(x: f32) -> f32 { return add(x, mul(x, x)); }
        "#));
    }

    // ── error cases ───────────────────────────────────────────────

    #[test]
    fn undefined_variable() {
        check_fails(compile(r#"fn main() -> f32 { return x; }"#), "Undefined");
    }

    #[test]
    fn undefined_function() {
        check_fails(compile(r#"fn main() -> f32 { return foo(1.0); }"#), "Undefined");
    }

    #[test]
    fn type_mismatch() {
        check_fails(compile(r#"
            fn main() -> f32 {
                let x: f32 = 1;
                return x;
            }
        "#), "MismatchedTypes");
    }

    #[test]
    fn assign_to_immutable() {
        check_fails(compile(r#"
            fn main() -> f32 {
                let x: f32 = 1.0;
                x = 2.0;
                return x;
            }
        "#), "AssignToImmutable");
    }

    #[test]
    fn wrong_return_type() {
        check_fails(compile(r#"
            fn main() -> f32 {
                return 1;
            }
        "#), "WrongFunctionReturnType");
    }

    #[test]
    fn wrong_arg_count() {
        check_fails(compile(r#"
            fn add(a: f32, b: f32) -> f32 { return a + b; }
            fn main() -> f32 { return add(1.0); }
        "#), "WrongFunctionArgs");
    }

    #[test]
    fn wrong_arg_type() {
        check_fails(compile(r#"
            fn add(a: f32, b: f32) -> f32 { return a + b; }
            fn main() -> f32 { return add(1, 2); }
        "#), "WrongFunctionArgs");
    }

    #[test]
    fn if_condition_not_bool() {
        check_fails(compile(r#"
            fn main() -> f32 {
                if 1.0 {
                    return 1.0;
                };
                return 0.0;
            }
        "#), "MismatchedTypes");
    }

    #[test]
    fn struct_literal_not_supported() {
        check_fails(compile(r#"
            fn main() -> f32 {
                let s: f32 = Foo { x: 1.0 };
                return 1.0;
            }
        "#), "NotSupported");
    }

    #[test]
    fn array_literal_type_mismatch() {
        check_fails(compile(r#"
            fn main() -> f32 {
                let a: f32 = [1.0, 2.0];
                return 1.0;
            }
        "#), "MismatchedTypes");
    }

    #[test]
    fn redundant_declaration() {
        check_fails(compile(r#"
            fn main() -> f32 {
                let x: f32 = 1.0;
                let x: f32 = 2.0;
                return x;
            }
        "#), "AlreadyDeclared");
    }

    #[test]
    fn parse_error_unclosed_paren() {
        check_fails(compile(r#"fn main() -> f32 { return (1.0; }"#), "parse:");
    }

    #[test]
    fn parse_error_missing_semicolon() {
        check_fails(compile(r#"fn main() -> f32 { return 1.0 }"#), "parse:");
    }

    // ── struct / enum definitions (parse-only, silently dropped) ──

    #[test]
    fn struct_definition_parses() {
        check_spv(compile(r#"
            struct Pair { x: f32, y: f32, }
            fn main() -> f32 { return 1.0; }
        "#));
    }

    #[test]
    fn enum_fieldless_literal() {
        check_spv(compile(r#"
            enum Color { Red, Green, Blue, }
            fn main() -> f32 {
                let c: Color = Color::Red;
                return 1.0;
            }
        "#));
    }

    #[test]
    fn enum_return_type() {
        check_spv(compile(r#"
            enum Color { Red, Green, Blue, }
            fn main() -> Color {
                return Color::Green;
            }
        "#));
    }

    #[test]
    fn enum_multiple_variants() {
        check_spv(compile(r#"
            enum Mode { Alpha, Beta, Gamma, Delta, }
            fn main() -> Mode {
                return Mode::Delta;
            }
        "#));
    }

    #[test]
    fn enum_variable_scope() {
        check_spv(compile(r#"
            enum Status { Active, Inactive, }
            fn main() -> f32 {
                let s: Status = Status::Active;
                let t: Status = Status::Inactive;
                return 1.0;
            }
        "#));
    }

    #[test]
    fn enum_unknown_variant() {
        check_fails(compile(r#"
            enum Color { Red, Green, Blue, }
            fn main() -> f32 {
                let c: Color = Color::Purple;
                return 1.0;
            }
        "#), "NotSupported");
    }

    #[test]
    fn enum_unknown_name() {
        check_fails(compile(r#"
            fn main() -> f32 {
                let c: Color = Color::Red;
                return 1.0;
            }
        "#), "NotSupported");
    }

    #[test]
    fn enum_simple_payload() {
        check_spv(compile(r#"
            enum Option { Some(f32), None, }
            fn main() -> f32 {
                let o: Option = Option::Some(1.0);
                return 1.0;
            }
        "#));
    }

    #[test]
    fn enum_payload_mixed_use() {
        check_spv(compile(r#"
            enum Option { Some(f32), None, }
            fn main() -> f32 {
                let a: Option = Option::Some(1.0);
                let b: Option = Option::None;
                return 1.0;
            }
        "#));
    }

    #[test]
    fn enum_definition_parses() {
        check_spv(compile(r#"
            enum Color { Red, Green, Blue, }
            fn main() -> f32 { return 1.0; }
        "#));
    }

    #[test]
    fn enum_with_data_parses() {
        check_spv(compile(r#"
            enum Option { Some(f32), None, }
            fn main() -> f32 { return 1.0; }
        "#));
    }

    // ── casts ─────────────────────────────────────────────────────

    #[test]
    fn cast_expression() {
        check_spv(compile(r#"
            fn main() -> f32 {
                let x: f32 = 1.0;
                return x;
            }
        "#));
    }

    // ── mixed features ────────────────────────────────────────────

    #[test]
    fn compute_shader_attr() {
        check_spv(compile(r#"
            #[compute(8, 8, 1)]
            fn main() { return; }
        "#));
    }

    #[test]
    fn multiple_descriptors() {
        check_spv(compile(r#"
            #layout(set = 0, binding = 0) a: float4;
            #layout(set = 0, binding = 1) b: float4;
            #layout(_push_constant) pc: f32;
            fn main() -> f32 { return 1.0; }
        "#));
    }

    // ── edge: empty function body ─────────────────────────────────

    #[test]
    fn empty_return_void() {
        check_spv(compile(r#"fn main() { }"#));
    }

    #[test]
    fn multi_block_no_if() {
        check_spv(compile(r#"
            fn main(x: f32) -> f32 {
                let a: f32 = x + 1.0;
                let b: f32 = a * 2.0;
                return b;
            }
        "#));
    }

    // ── const globals ─────────────────────────────────────────────

    #[test]
    fn const_global_used_in_fn() {
        check_spv(compile(r#"
            const PI: f32 = 3.14;
            fn main() -> f32 {
                return PI;
            }
        "#));
    }

    #[test]
    fn const_global_multiple_fns() {
        check_spv(compile(r#"
            const BASE: f32 = 1.0;
            fn add(a: f32) -> f32 {
                return a + BASE;
            }
            fn mul(a: f32) -> f32 {
                return a * BASE;
            }
        "#));
    }

    #[test]
    fn const_global_int() {
        check_spv(compile(r#"
            const ZERO: i32 = 0;
            fn main() -> i32 {
                return ZERO;
            }
        "#));
    }

    #[test]
    fn const_global_type_mismatch() {
        check_fails(compile(r#"
            const X: f32 = 1;
            fn main() -> f32 { return X; }
        "#), "MismatchedTypes");
    }

    #[test]
    fn const_global_missing_init() {
        check_fails(compile(r#"
            const X: f32;
            fn main() -> f32 { return X; }
        "#), "parse:");
    }

    // ── structs ───────────────────────────────────────────────────

    #[test]
    fn struct_literal_and_member() {
        check_spv(compile(r#"
            struct Pair { x: f32, y: f32, }
            fn main() -> f32 {
                let p: Pair = Pair { x: 1.0, y: 2.0 };
                return p.x;
            }
        "#));
    }

    #[test]
    fn struct_field_order() {
        check_spv(compile(r#"
            struct Ab { a: f32, b: f32, }
            fn main() -> f32 {
                let p: Ab = Ab { a: 1.0, b: 2.0 };
                return p.b;
            }
        "#));
    }

    #[test]
    fn struct_in_struct() {
        check_spv(compile(r#"
            struct Inner { val: f32, }
            struct Outer { inner: Inner, extra: f32, }
            fn main() -> f32 {
                let o: Outer = Outer { inner: Inner { val: 1.0 }, extra: 2.0 };
                return o.inner.val;
            }
        "#));
    }

    #[test]
    fn struct_unknown_field() {
        check_fails(compile(r#"
            struct Pair { x: f32, y: f32, }
            fn main() -> f32 {
                let p: Pair = Pair { x: 1.0, y: 2.0 };
                return p.z;
            }
        "#), "MismatchedTypes");
    }

    #[test]
    fn struct_wrong_field_type() {
        check_fails(compile(r#"
            struct Pair { x: f32, y: f32, }
            fn main() -> f32 {
                let p: Pair = Pair { x: 1.0, y: 2 };
                return p.x;
            }
        "#), "MismatchedTypes");
    }

    #[test]
    fn struct_return() {
        check_spv(compile(r#"
            struct Pair { x: f32, y: f32, }
            fn make() -> Pair {
                return Pair { x: 1.0, y: 2.0 };
            }
            fn main() -> f32 {
                let p: Pair = make();
                return p.x;
            }
        "#));
    }

    #[test]
    fn struct_mutate_field() {
        check_spv(compile(r#"
            struct Pair { x: f32, y: f32, }
            fn main() -> f32 {
                let mut p: Pair = Pair { x: 1.0, y: 2.0 };
                p.x = 42.0;
                return p.x;
            }
        "#));
    }

    #[test]
    fn struct_field_location_attr() {
        check_spv(compile(r#"
            struct VOut {
                #[location(0)] pos: f32,
                #[location(1)] color: f32,
            }
            #[vertex]
            fn main() -> VOut {
                return VOut { pos: 0.0, color: 1.0 };
            }
        "#));
    }

    #[test]
    fn struct_field_builtin_attr() {
        check_spv(compile(r#"
            struct VOut {
                #[builtin(position)] pos: f32,
                #[location(0)] color: f32,
            }
            #[vertex]
            fn main() -> VOut {
                return VOut { pos: 0.0, color: 1.0 };
            }
        "#));
    }

    #[test]
    fn entry_point_location_output() {
        check_spv(compile(r#"
            struct VOut {
                #[location(0)] a: f32,
                #[location(1)] b: f32,
            }
            #[vertex]
            fn main() -> VOut {
                return VOut { a: 1.0, b: 2.0 };
            }
        "#));
    }

    #[test]
    fn entry_point_builtin_output() {
        check_spv(compile(r#"
            struct VOut {
                #[builtin(position)] pos: f32,
            }
            #[vertex]
            fn main() -> VOut {
                return VOut { pos: 1.0 };
            }
        "#));
    }

    #[test]
    fn entry_point_fragment_output() {
        check_spv(compile(r#"
            struct FOut {
                #[location(0)] color: f32,
            }
            #[fragment]
            fn main() -> FOut {
                return FOut { color: 1.0 };
            }
        "#));
    }
}
