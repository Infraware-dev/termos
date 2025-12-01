/// SCAN Algorithm Performance Benchmarks
///
/// This benchmark suite measures classification performance for different input types.
/// Target: <50μs per classification (10x improvement over baseline)
use criterion::{criterion_group, criterion_main, Criterion};
use infraware_terminal::input::handler::{
    ApplicationBuiltinHandler, ClassifierContext, CommandSyntaxHandler, DefaultHandler,
    EmptyInputHandler, InputHandler, KnownCommandHandler, NaturalLanguageHandler,
    PathCommandHandler, PathDiscoveryHandler,
};
use infraware_terminal::input::shell_builtins::ShellBuiltinHandler;
use infraware_terminal::input::typo_detection::TypoDetectionHandler;
use infraware_terminal::input::InputClassifier;
use std::hint::black_box;

fn benchmark_classification(c: &mut Criterion) {
    let classifier = InputClassifier::new();

    c.bench_function("classify_known_command", |b| {
        b.iter(|| classifier.classify(black_box("docker ps -a")));
    });

    c.bench_function("classify_natural_language", |b| {
        b.iter(|| classifier.classify(black_box("how do I list running containers?")));
    });

    c.bench_function("classify_command_with_flags", |b| {
        b.iter(|| classifier.classify(black_box("kubectl get pods --all-namespaces")));
    });

    c.bench_function("classify_multilingual_question", |b| {
        b.iter(|| classifier.classify(black_box("come posso listare i file?")));
    });

    c.bench_function("classify_empty_input", |b| {
        b.iter(|| classifier.classify(black_box("")));
    });

    c.bench_function("classify_single_word_command", |b| {
        b.iter(|| classifier.classify(black_box("htop")));
    });

    c.bench_function("classify_executable_path", |b| {
        b.iter(|| classifier.classify(black_box("./deploy.sh --production")));
    });

    c.bench_function("classify_command_with_pipes", |b| {
        b.iter(|| classifier.classify(black_box("cat file.txt | grep pattern | wc -l")));
    });
}

/// Benchmark individual handlers for performance profiling
///
/// This helps identify which handlers are bottlenecks in the classification chain.
/// Target performance for each handler:
/// - EmptyInputHandler: <1μs
/// - ApplicationBuiltinHandler: <1μs
/// - ShellBuiltinHandler: <1μs
/// - PathCommandHandler: ~10μs (file system check)
/// - KnownCommandHandler: <1μs (cache hit)
/// - PathDiscoveryHandler: <1μs (cache hit)
/// - CommandSyntaxHandler: ~10μs (regex matching)
/// - TypoDetectionHandler: ~100μs (Levenshtein distance)
/// - NaturalLanguageHandler: ~0.5μs (heuristics)
/// - DefaultHandler: <1μs
fn benchmark_individual_handlers(c: &mut Criterion) {
    let ctx = ClassifierContext::new();

    // 1. EmptyInputHandler - fastest path
    let empty_handler = EmptyInputHandler::new();
    c.bench_function("handler_empty_input", |b| {
        b.iter(|| empty_handler.handle(black_box("   "), &ctx));
    });

    // 3. ApplicationBuiltinHandler
    let app_builtin_handler = ApplicationBuiltinHandler::new();
    c.bench_function("handler_app_builtin_clear", |b| {
        b.iter(|| app_builtin_handler.handle(black_box("clear"), &ctx));
    });
    c.bench_function("handler_app_builtin_reload", |b| {
        b.iter(|| app_builtin_handler.handle(black_box("reload-aliases"), &ctx));
    });

    // 4. ShellBuiltinHandler
    let shell_builtin_handler = ShellBuiltinHandler::new();
    c.bench_function("handler_shell_builtin_export", |b| {
        b.iter(|| shell_builtin_handler.handle(black_box("export VAR=value"), &ctx));
    });
    c.bench_function("handler_shell_builtin_source", |b| {
        b.iter(|| shell_builtin_handler.handle(black_box("source ~/.bashrc"), &ctx));
    });

    // 5. PathCommandHandler
    let path_handler = PathCommandHandler::new();
    c.bench_function("handler_path_relative", |b| {
        b.iter(|| path_handler.handle(black_box("./deploy.sh --prod"), &ctx));
    });
    c.bench_function("handler_path_absolute", |b| {
        b.iter(|| path_handler.handle(black_box("/usr/bin/python3 script.py"), &ctx));
    });

    // 6. KnownCommandHandler (cache hit scenario)
    let known_handler = KnownCommandHandler::with_defaults();
    c.bench_function("handler_known_command_ls", |b| {
        b.iter(|| known_handler.handle(black_box("ls -la"), &ctx));
    });
    c.bench_function("handler_known_command_docker", |b| {
        b.iter(|| known_handler.handle(black_box("docker ps"), &ctx));
    });

    // 7. PathDiscoveryHandler (cache hit scenario)
    let discovery_handler = PathDiscoveryHandler::new();
    c.bench_function("handler_path_discovery", |b| {
        b.iter(|| discovery_handler.handle(black_box("python3 --version"), &ctx));
    });

    // 8. CommandSyntaxHandler
    let syntax_handler = CommandSyntaxHandler::new();
    c.bench_function("handler_syntax_flags", |b| {
        b.iter(|| syntax_handler.handle(black_box("unknown-cmd --flag value"), &ctx));
    });
    c.bench_function("handler_syntax_pipe", |b| {
        b.iter(|| syntax_handler.handle(black_box("cat file | grep pattern"), &ctx));
    });

    // 9. TypoDetectionHandler (disabled by default, max_distance=0)
    let typo_handler = TypoDetectionHandler::with_defaults();
    c.bench_function("handler_typo_disabled", |b| {
        b.iter(|| typo_handler.handle(black_box("dokcer ps"), &ctx));
    });

    // 9b. TypoDetectionHandler with typo detection enabled (max_distance=2)
    let typo_handler_enabled = TypoDetectionHandler::from_config(
        infraware_terminal::input::known_commands::default_devops_commands(),
        2,
        &ctx.language_patterns,
    );
    c.bench_function("handler_typo_enabled_match", |b| {
        b.iter(|| typo_handler_enabled.handle(black_box("dokcer ps"), &ctx));
    });
    c.bench_function("handler_typo_enabled_no_match", |b| {
        b.iter(|| typo_handler_enabled.handle(black_box("how do I list files"), &ctx));
    });

    // 10. NaturalLanguageHandler
    let nl_handler = NaturalLanguageHandler::new();
    c.bench_function("handler_nl_question", |b| {
        b.iter(|| nl_handler.handle(black_box("how do I list running containers?"), &ctx));
    });
    c.bench_function("handler_nl_long_phrase", |b| {
        b.iter(|| nl_handler.handle(black_box("show me the docker containers"), &ctx));
    });
    c.bench_function("handler_nl_multilingual", |b| {
        b.iter(|| nl_handler.handle(black_box("come posso vedere i container?"), &ctx));
    });

    // 11. DefaultHandler
    let default_handler = DefaultHandler::new();
    c.bench_function("handler_default_fallback", |b| {
        b.iter(|| default_handler.handle(black_box("unknown input"), &ctx));
    });
}

criterion_group!(
    benches,
    benchmark_classification,
    benchmark_individual_handlers
);
criterion_main!(benches);
