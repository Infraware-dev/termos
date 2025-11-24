/// SCAN Algorithm Performance Benchmarks
///
/// This benchmark suite measures classification performance for different input types.
/// Target: <50μs per classification (10x improvement over baseline)
use criterion::{criterion_group, criterion_main, Criterion};
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

criterion_group!(benches, benchmark_classification);
criterion_main!(benches);
