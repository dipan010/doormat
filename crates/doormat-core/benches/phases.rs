//! Per-phase criterion benchmarks for doormat pipeline profiling.
//!
//! Each benchmark isolates a single pipeline phase so we can identify
//! the actual bottleneck. Uses `iter_batched` with `SmallInput` to
//! avoid timing the setup code.

mod fixtures;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

use doormat_core::candidates::{classify_cells, reduce_candidate_space};
use doormat_core::detection::{analyze_comments, analyze_formulas, detect_inline_credentials};
use doormat_core::features::precompute_features;
use doormat_core::inference::infer_relationships;
use doormat_core::pipeline::deduplicate;
use doormat_core::regions::detect_regions;
use doormat_core::spatial::DISTANCE_TABLE;
use doormat_core::store::{CellStore, RawCell};
use doormat_core::types::Relationship;

/// Build a CellStore with features, candidates, classification and regions done.
fn build_full_pipeline_store(cells: Vec<RawCell>) -> (CellStore, Vec<u32>) {
    let mut store = CellStore::from_raw(cells);
    precompute_features(&mut store);
    let candidates = reduce_candidate_space(&store);
    classify_cells(&mut store, &candidates);
    detect_regions(&mut store, &candidates);
    (store, candidates)
}

// ---------- Phase benchmarks ----------

fn bench_from_raw(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_raw");

    let tiny = fixtures::tiny_cells();
    group.bench_with_input(BenchmarkId::new("cells", "tiny"), &tiny, |b, cells| {
        b.iter_batched(|| cells.clone(), CellStore::from_raw, BatchSize::SmallInput)
    });

    let typical = fixtures::typical_cells();
    group.bench_with_input(
        BenchmarkId::new("cells", "typical"),
        &typical,
        |b, cells| b.iter_batched(|| cells.clone(), CellStore::from_raw, BatchSize::SmallInput),
    );

    let large = fixtures::large_cells();
    group.bench_with_input(BenchmarkId::new("cells", "large"), &large, |b, cells| {
        b.iter_batched(|| cells.clone(), CellStore::from_raw, BatchSize::SmallInput)
    });

    group.finish();
}

fn bench_precompute_features(c: &mut Criterion) {
    let mut group = c.benchmark_group("precompute_features");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let cells = gen();
        group.bench_with_input(BenchmarkId::new("cells", name), &cells, |b, cells| {
            b.iter_batched(
                || CellStore::from_raw(cells.clone()),
                |mut store| {
                    precompute_features(&mut store);
                    store
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

fn bench_reduce_candidates(c: &mut Criterion) {
    let mut group = c.benchmark_group("reduce_candidates");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let store = fixtures::build_store(gen());
        group.bench_with_input(BenchmarkId::new("cells", name), &store, |b, store| {
            b.iter(|| reduce_candidate_space(store))
        });
    }

    group.finish();
}

fn bench_classify_cells(c: &mut Criterion) {
    let mut group = c.benchmark_group("classify_cells");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let base_store = fixtures::build_store(gen());
        let candidates = reduce_candidate_space(&base_store);
        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &(base_store, candidates),
            |b, (store, cands)| {
                b.iter_batched(
                    || store.clone(),
                    |mut s| {
                        classify_cells(&mut s, cands);
                        s
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

fn bench_detect_regions(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_regions");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let mut store = fixtures::build_store(gen());
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &(store, candidates),
            |b, (store, cands)| {
                b.iter_batched(
                    || store.clone(),
                    |mut s| {
                        detect_regions(&mut s, cands);
                        s
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

fn bench_detect_inline(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_inline");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let (store, candidates) = build_full_pipeline_store(gen());
        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &(store, candidates),
            |b, (store, cands)| b.iter(|| detect_inline_credentials(store, cands)),
        );
    }

    group.finish();
}

fn bench_analyze_formulas(c: &mut Criterion) {
    let mut group = c.benchmark_group("analyze_formulas");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let (store, candidates) = build_full_pipeline_store(gen());
        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &(store, candidates),
            |b, (store, cands)| b.iter(|| analyze_formulas(store, cands)),
        );
    }

    group.finish();
}

fn bench_analyze_comments(c: &mut Criterion) {
    let mut group = c.benchmark_group("analyze_comments");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let (store, candidates) = build_full_pipeline_store(gen());
        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &(store, candidates),
            |b, (store, cands)| b.iter(|| analyze_comments(store, cands)),
        );
    }

    group.finish();
}

fn bench_infer_relationships(c: &mut Criterion) {
    let mut group = c.benchmark_group("infer_relationships");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let (store, _candidates) = build_full_pipeline_store(gen());
        group.bench_with_input(BenchmarkId::new("cells", name), &store, |b, store| {
            b.iter(|| infer_relationships(store, &DISTANCE_TABLE))
        });
    }

    group.finish();
}

fn bench_deduplicate(c: &mut Criterion) {
    let mut group = c.benchmark_group("deduplicate");

    // Build some representative relationships to deduplicate
    for (name, count) in [("tiny", 5usize), ("typical", 50), ("large", 500)] {
        let rels: Vec<Relationship> = (0..count)
            .map(|i| Relationship {
                header_cell_id: i,
                value_cell_id: i + 1,
                key: format!("key_{}", i % 10),
                value: format!("value_{i}"),
                confidence: 150.0 + (i % 50) as f32,
                reason: "distance=100;upper;lower".into(),
            })
            .collect();

        group.bench_with_input(BenchmarkId::new("rels", name), &rels, |b, rels| {
            b.iter_batched(|| rels.clone(), deduplicate, BatchSize::SmallInput)
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_from_raw,
    bench_precompute_features,
    bench_reduce_candidates,
    bench_classify_cells,
    bench_detect_regions,
    bench_detect_inline,
    bench_analyze_formulas,
    bench_analyze_comments,
    bench_infer_relationships,
    bench_deduplicate,
);
criterion_main!(benches);
