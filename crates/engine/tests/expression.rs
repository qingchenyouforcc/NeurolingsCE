use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use engine::{Engine, EvalContext, EvalError, Expression, Value};

fn object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn evaluates_number_boolean_and_string_literals() {
    let mut context = EvalContext::default();

    assert_eq!(
        Expression::parse("42")
            .unwrap()
            .evaluate(&mut context)
            .unwrap(),
        Value::Number(42.0)
    );
    assert_eq!(
        Expression::parse("true")
            .unwrap()
            .evaluate(&mut context)
            .unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        Expression::parse("'desk pet'")
            .unwrap()
            .evaluate(&mut context)
            .unwrap(),
        Value::String("desk pet".to_owned())
    );
}

#[test]
fn resolves_nested_member_access() {
    let mut context = EvalContext::default();
    context.set_variable(
        "mascot",
        object([("anchor", object([("x", Value::Number(3.0))]))]),
    );

    let value = Expression::parse("mascot.anchor.x + 2")
        .unwrap()
        .evaluate(&mut context)
        .unwrap();

    assert_eq!(value, Value::Number(5.0));
}

#[test]
fn invokes_registered_functions() {
    let mut context = EvalContext::default();
    context.register_function("double", |args| {
        let value = args.first().ok_or_else(|| EvalError::Arity {
            name: "double".to_owned(),
            expected: 1,
            actual: 0,
        })?;
        Ok(Value::Number(
            value.as_number().ok_or_else(|| EvalError::TypeMismatch {
                expected: "number",
                actual: value.type_name(),
            })? * 2.0,
        ))
    });

    assert_eq!(
        Expression::parse("double(4)")
            .unwrap()
            .evaluate(&mut context)
            .unwrap(),
        Value::Number(8.0)
    );
}

#[test]
fn selects_ternary_branch_using_truthiness() {
    let mut context = EvalContext::default();
    context.set_variable("visible", Value::Bool(false));

    let value = Expression::parse("visible ? 'show' : 'hide'")
        .unwrap()
        .evaluate(&mut context)
        .unwrap();

    assert_eq!(value, Value::String("hide".to_owned()));
}

#[test]
fn short_circuit_logic_does_not_invoke_skipped_branch() {
    let mut context = EvalContext::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_function = Arc::clone(&calls);
    context.register_function("side_effect", move |_| {
        calls_for_function.fetch_add(1, Ordering::SeqCst);
        Ok(Value::Bool(true))
    });

    assert_eq!(
        Expression::parse("false && side_effect()")
            .unwrap()
            .evaluate(&mut context)
            .unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        Expression::parse("true || side_effect()")
            .unwrap()
            .evaluate(&mut context)
            .unwrap(),
        Value::Bool(true)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn injects_deterministic_math_random_source() {
    let mut context = EvalContext::default();
    context.set_random_source(|| 0.25);

    let value = Expression::parse("10 + Math.random() * 4")
        .unwrap()
        .evaluate(&mut context)
        .unwrap();

    assert_eq!(value, Value::Number(11.0));
}

#[test]
fn engine_evaluates_source_and_reports_invalid_expressions() {
    let mut engine = Engine::default();
    assert_eq!(engine.evaluate_source("1 + 2").unwrap(), Value::Number(3.0));

    assert!(Expression::parse("1 +").is_err());
    assert!(
        matches!(engine.evaluate_source("missing"), Err(engine::EngineError::Eval(EvalError::UndefinedVariable(name))) if name == "missing")
    );
}
