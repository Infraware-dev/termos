---
name: microsoft-rust-guidelines
description: Apply Microsoft's Pragmatic Rust Guidelines when writing or reviewing Rust code. These are enterprise-scale best practices from Microsoft covering safety, performance, readability, and idiomatic patterns.
---

# Microsoft Pragmatic Rust Guidelines

Source: https://microsoft.github.io/rust-guidelines/

## Meta Design Principles

All guidelines satisfy these criteria:
- **Safety & Performance**: Promote safety best-practices, prevent risk, achieve high throughput/low latency/low memory
- **Readability**: Make code readable and understandable
- **Community Agreement**: Must align with experienced developers (3+ years)
- **Accessibility**: Comprehensible to Rust novices (4+ weeks experience)
- **Pragmatism**: Must be realistic and followable

## Golden Rule

"Each item exists for a reason; the spirit counts, not the letter." Understand the *why* behind guidelines. Question them when they conflict with underlying motivations.

---

## Universal Guidelines

### M-UPSTREAM-GUIDELINES: Follow Upstream Guidelines

**Always follow:**
- Rust API Guidelines
- Rust Style Guide
- Rust Design Patterns
- Rust Reference - Undefined Behavior

**Key conventions:**
- Ad-hoc conversions: `as_`, `to_`, `into_` (C-CONV)
- Getters follow Rust conventions (C-GETTER)
- Implement common traits: `Copy`, `Clone`, `Eq`, `PartialEq`, `Ord`, `PartialOrd`, `Hash`, `Default`, `Debug` (C-COMMON-TRAITS)
- Constructors are static: include `Foo::new()` even with `Default` (C-CTOR)
- Feature names avoid placeholder words (C-FEATURE)

---

### M-STATIC-VERIFICATION: Use Static Verification

**Required compiler lints:**
- `ambiguous_negative_literals`
- `missing_debug_implementations`
- `redundant_imports`
- `redundant_lifetimes`
- `trivial_numeric_casts`
- `unsafe_op_in_unsafe_fn`
- `unused_lifetimes`

**Required Clippy lints:**
Enable: `cargo`, `complexity`, `correctness`, `pedantic`, `perf`, `style`, `suspicious` + restriction lints

**Required tools:**
- `rustfmt` - consistent formatting
- `cargo-audit` - dependency vulnerability scanning
- `cargo-hack` - feature combination validation
- `cargo-udeps` - unused dependency detection
- `miri` - unsafe code validation

---

### M-LINT-OVERRIDE-EXPECT: Use #[expect] for Lint Overrides

Use `#[expect]` instead of `#[allow]` to prevent accumulation of outdated lints.

```rust
#[expect(clippy::unused_async, reason = "API fixed, will use I/O later")]
pub async fn ping_server() {
    // Stubbed out
}
```

Exception: `#[allow]` acceptable for generated code/macros.

---

### M-PUBLIC-DEBUG: Public Types are Debug

All public types MUST implement `Debug`. Use `#[derive(Debug)]` when possible:

```rust
#[derive(Debug)]
struct Endpoint(String);
```

For sensitive data, implement custom `Debug`:

```rust
impl Debug for UserSecret {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "UserSecret(...)")
    }
}
```

Test that sensitive data isn't leaked.

---

### M-PUBLIC-DISPLAY: Public Types Meant to be Read are Display

Implement `Display` for user-facing types:
- Error types (required by `std::error::Error`)
- Wrappers around string-like data

Follow Rust conventions for newlines/escape sequences. Handle sensitive data like `Debug`.

---

### M-SMALLER-CRATES: If in Doubt, Split the Crate

Prefer more crates over fewer. Move independently usable submodules into separate crates.

**Benefits:**
- Better compile times
- Prevents cyclic dependencies
- Improves modularity

**Rule:** Crates = items usable independently; Features = unlock extra functionality.

---

### M-CONCISE-NAMES: Names are Free of Weasel Words

Avoid vague names like `Service`, `Manager`, `Factory`:
- ❌ `BookingService` → ✅ `Bookings`
- ❌ `BookingManager` → ✅ Specific functional name
- ❌ `FooFactory` → ✅ `Builder`

Avoid passing factories/builders as parameters; use `impl Fn() -> Foo`.

---

### M-REGULAR-FN: Prefer Regular over Associated Functions

Associate functions primarily with instance creation. Use regular functions for general computation:

```rust
impl Database {
    fn new() -> Self {}        // ✅ Creates instance
    fn query(&self) {}         // ✅ Uses &self
}

// ❌ Don't do this as associated function
// fn check_parameters(p: &str) {}

// ✅ Use regular function instead
fn check_parameters(p: &str) {}
```

---

### M-PANIC-IS-STOP: Panic Means 'Stop the Program'

Panics = immediate program termination, NOT exceptions.

**Invalid uses:**
- Error communication
- Self-inflicted error handling
- Assuming catches

**Valid reasons:**
- Programming errors (`x.expect("must never happen")`)
- Const contexts
- User-requested unwrapping
- Encountering poison (e.g., poisoned locks)

---

### M-PANIC-ON-BUG: Detected Programming Bugs are Panics, Not Errors

Unrecoverable programming errors MUST panic; never return `Error` for contract violations.

```rust
// ✅ Should panic (clear violation)
fn divide_by(x: i32, y: i32) {
    assert!(y != 0, "division by zero");
}

// ✅ Should return Result (inherently fallible)
fn parse_uri(s: &str) -> Result<Uri, ParseError> {
    // ...
}
```

---

### M-DOCUMENTED-MAGIC: Magic Values are Documented

All hardcoded values require comments explaining:
- Why chosen
- Side effects of changing
- External dependencies

Prefer named constants:

```rust
/// How long we wait for the server.
/// Large enough to ensure completion. Too-low values abort valid requests.
/// Based on `api.foo.com` policies.
const UPSTREAM_SERVER_TIMEOUT: Duration = Duration::from_secs(60 * 60 * 24);
```

---

### M-LOG-STRUCTURED: Use Structured Logging with Message Templates

Follow https://messagetemplates.org/ specification.

**Avoid string formatting:**

```rust
// ❌ Bad
tracing::info!("file opened: {}", path);

// ✅ Good
event!(
    name: "file.open.success",
    Level::INFO,
    file.path = path.display(),
    "file opened: {{file.path}}",
);
```

**Event naming:** `<component>.<operation>.<state>`

**Follow OpenTelemetry conventions:** Use standard attribute names (`http.request.method`, `file.path`, `db.system.name`)

**Redact sensitive data:** Never log emails, identifying file paths, tokens, or PII.

---

## Safety Guidelines

### M-UNSAFE: Unsafe Code Must Have Justification

**Only 3 valid reasons:**
1. Novel abstractions (new smart pointers, allocators)
2. Performance optimization (e.g., unchecked access)
3. FFI and platform interactions

### When NOT to Use Unsafe:
- ❌ Shortening safe, performant code
- ❌ Bypassing trait bounds like `Send`
- ❌ Circumventing lifetimes via `transmute`

### Requirements for Novel Abstractions:
- Verify no alternatives exist
- Keep minimal and testable
- Harden against adversarial patterns
- Include plain-text safety explanations
- Pass Miri testing (including adversarial cases)
- Follow official unsafe guidelines

### Performance-Related Unsafe:
- Benchmark FIRST
- Document safety reasoning
- Pass Miri validation
- Follow unsafe guidelines

---

### M-UNSOUND: All Code Must Be Sound

"Unsound code is seemingly safe code that may produce undefined behavior when called from other safe code."

**Absolute rule - NO exceptions.** If safe encapsulation impossible, expose `unsafe` functions with clear documentation.

---

### M-UNSAFE-IMPLIES-UB: Unsafe Implies Undefined Behavior

`unsafe` marker applies ONLY when misuse risks undefined behavior.

❌ Don't mark general danger as unsafe: `delete_database()` shouldn't be `unsafe`

---

## Applying These Guidelines

When writing Rust code:

1. **Before coding:**
   - Check Rust API Guidelines & Clippy for existing patterns
   - Prefer splitting crates over monoliths
   - Avoid `unsafe` unless one of 3 valid reasons applies

2. **During coding:**
   - Use clear, concise names (no weasel words)
   - Implement `Debug` for all public types
   - Prefer regular functions over associated (unless instance-related)
   - Panic for programming errors, `Result` for expected failures
   - Document all magic values
   - Use structured logging with templates

3. **After coding:**
   - Enable all required compiler/Clippy lints
   - Run `rustfmt`, `cargo-audit`, `cargo-udeps`
   - If using `unsafe`, run `miri`
   - Use `#[expect]` instead of `#[allow]`
   - Test that `Debug` doesn't leak sensitive data

4. **Code review:**
   - Verify panic vs error handling appropriateness
   - Check for weasel words in names
   - Ensure all public types have `Debug`
   - Validate `unsafe` usage justification
   - Confirm structured logging patterns

---

## Quick Reference Checklist

- [ ] Follows Rust API Guidelines (C-CONV, C-GETTER, C-COMMON-TRAITS, C-CTOR)
- [ ] All required lints enabled (ambiguous_negative_literals, missing_debug_implementations, etc.)
- [ ] Uses `#[expect]` instead of `#[allow]`
- [ ] All public types implement `Debug` (custom for sensitive data)
- [ ] User-facing types implement `Display`
- [ ] Independently usable modules in separate crates
- [ ] No weasel words (Service, Manager, Factory) in names
- [ ] Associated functions only for instance creation/methods
- [ ] Panics for programming errors, `Result` for expected failures
- [ ] Magic values documented with rationale
- [ ] Structured logging with message templates
- [ ] `unsafe` usage justified (novel abstraction/performance/FFI only)
- [ ] All code is sound (no UB from safe code)
- [ ] Miri passes for unsafe code

---

**Remember:** The spirit counts, not the letter. Understand WHY each guideline exists and apply pragmatically.
