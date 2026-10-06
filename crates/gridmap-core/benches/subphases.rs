//! Sub-phase benchmarks for `from_raw` and `detect_regions`.
//!
//! Breaks each function into measurable internal steps to identify
//! the dominant sub-phase within each pipeline phase.

mod fixtures;

use ahash::{AHashMap, AHashSet};
use arrow2::array::{BooleanArray, UInt32Array, Utf8Array};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};

use gridmap_core::candidates::{classify_cells, reduce_candidate_space};
use gridmap_core::regions::detect_regions;
use gridmap_core::spatial::query_radius;
use gridmap_core::store::{CellStore, RawCell};
use gridmap_core::types::NEIGHBOR_RADIUS;

// =====================================================================
// from_raw sub-phases
// =====================================================================

/// Benchmark the Vec collection + comment merge dedup pass (phase 1 of from_raw).
/// This isolates the loop that builds temp Vecs and the coord_to_id map.
fn bench_raw_collect_and_dedup(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_raw/collect_dedup");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let cells = gen();
        group.bench_with_input(BenchmarkId::new("cells", name), &cells, |b, cells| {
            b.iter_batched(
                || cells.clone(),
                |cells| {
                    let cap = cells.len();
                    let mut t_rows: Vec<u32> = Vec::with_capacity(cap);
                    let mut t_cols: Vec<u32> = Vec::with_capacity(cap);
                    let mut t_values: Vec<String> = Vec::with_capacity(cap);
                    let mut t_formulas: Vec<String> = Vec::with_capacity(cap);
                    let mut t_comments: Vec<String> = Vec::with_capacity(cap);
                    let mut t_sheet_names: Vec<String> = Vec::with_capacity(cap);
                    let mut t_merged: Vec<bool> = Vec::with_capacity(cap);
                    let mut coord_to_id: AHashMap<(u32, u32), u32> = AHashMap::with_capacity(cap);

                    for cell in cells {
                        let key = (cell.row, cell.col);
                        if let Some(&existing_id) = coord_to_id.get(&key) {
                            let id = existing_id as usize;
                            if !cell.comment.is_empty() {
                                let existing = &mut t_comments[id];
                                if existing.is_empty() {
                                    *existing = cell.comment;
                                } else {
                                    existing.push('\n');
                                    existing.push_str(&cell.comment);
                                }
                            }
                            continue;
                        }
                        let id = t_rows.len() as u32;
                        coord_to_id.insert(key, id);
                        t_rows.push(cell.row);
                        t_cols.push(cell.col);
                        t_values.push(cell.value);
                        t_formulas.push(cell.formula);
                        t_comments.push(cell.comment);
                        t_sheet_names.push(cell.sheet_name);
                        t_merged.push(cell.is_merged_origin);
                    }
                    (
                        t_rows,
                        t_cols,
                        t_values,
                        t_formulas,
                        t_comments,
                        t_sheet_names,
                        t_merged,
                        coord_to_id,
                    )
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

/// Benchmark Arrow array construction from pre-built Vecs.
fn bench_raw_to_arrow_arrays(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_raw/arrow_convert");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        // Pre-build the Vecs (the collect phase output)
        let cells = gen();
        let n = cells.len();
        let t_rows: Vec<u32> = cells.iter().map(|c| c.row).collect();
        let t_cols: Vec<u32> = cells.iter().map(|c| c.col).collect();
        let t_values: Vec<String> = cells.iter().map(|c| c.value.clone()).collect();
        let t_formulas: Vec<String> = cells.iter().map(|c| c.formula.clone()).collect();
        let t_comments: Vec<String> = cells.iter().map(|c| c.comment.clone()).collect();
        let t_sheet_names: Vec<String> = cells.iter().map(|c| c.sheet_name.clone()).collect();
        let t_merged: Vec<bool> = cells.iter().map(|c| c.is_merged_origin).collect();

        let input = (
            t_rows,
            t_cols,
            t_values,
            t_formulas,
            t_comments,
            t_sheet_names,
            t_merged,
            n,
        );

        group.bench_with_input(BenchmarkId::new("cells", name), &input, |b, input| {
            let (rows, cols, vals, forms, comms, sheets, merged, _n) = input;
            b.iter(|| {
                let _r = UInt32Array::from_vec(rows.clone());
                let _c = UInt32Array::from_vec(cols.clone());
                let _v = Utf8Array::<i32>::from_iter_values(vals.iter().map(|s| s.as_str()));
                let _f = Utf8Array::<i32>::from_iter_values(forms.iter().map(|s| s.as_str()));
                let _cm = Utf8Array::<i32>::from_iter_values(comms.iter().map(|s| s.as_str()));
                let _sn = Utf8Array::<i32>::from_iter_values(sheets.iter().map(|s| s.as_str()));
                let _m = BooleanArray::from_slice(merged.clone());
            })
        });
    }

    group.finish();
}

/// Benchmark coord_to_id AHashMap construction alone.
fn bench_coord_index_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_raw/coord_index");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let cells = gen();
        let coords: Vec<(u32, u32)> = cells.iter().map(|c| (c.row, c.col)).collect();

        group.bench_with_input(BenchmarkId::new("cells", name), &coords, |b, coords| {
            b.iter(|| {
                let mut map: AHashMap<(u32, u32), u32> = AHashMap::with_capacity(coords.len());
                for (i, &key) in coords.iter().enumerate() {
                    map.insert(key, i as u32);
                }
                map
            })
        });
    }

    group.finish();
}

/// Benchmark workspace Vec allocation (the mutable pipeline workspace).
fn bench_workspace_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_raw/workspace_alloc");

    for (name, n) in [("tiny", 100usize), ("typical", 5_000), ("large", 50_000)] {
        group.bench_with_input(BenchmarkId::new("cells", name), &n, |b, &n| {
            b.iter(|| {
                let _cell_types = vec![2u8; n];
                let _normalized = vec![String::new(); n];
                let _flags = vec![0u16; n];
                let _entropy = vec![0.0f32; n];
                let _region_ids = vec![-1i32; n];
            })
        });
    }

    group.finish();
}

/// Benchmark from_raw with duplicate coords to stress the comment merge path.
fn bench_comment_merge_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_raw/comment_merge");

    // Generate cells where ~10% are duplicates with comments
    for (name, base_n) in [("tiny", 100usize), ("typical", 5_000), ("large", 50_000)] {
        let mut cells = fixtures::generate_raw_cells(base_n, 42);
        let dup_count = base_n / 10;
        for i in 0..dup_count {
            let orig = &cells[i];
            cells.push(RawCell {
                row: orig.row,
                col: orig.col,
                value: orig.value.clone(),
                formula: String::new(),
                comment: format!("dup_comment_{i}"),
                sheet_name: orig.sheet_name.clone(),
                is_merged_origin: false,
            });
        }

        group.bench_with_input(BenchmarkId::new("cells", name), &cells, |b, cells| {
            b.iter_batched(|| cells.clone(), CellStore::from_raw, BatchSize::SmallInput)
        });
    }

    group.finish();
}

/// Full from_raw benchmark for baseline comparison.
fn bench_from_raw_full(c: &mut Criterion) {
    let mut group = c.benchmark_group("from_raw/full");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let cells = gen();
        group.bench_with_input(BenchmarkId::new("cells", name), &cells, |b, cells| {
            b.iter_batched(|| cells.clone(), CellStore::from_raw, BatchSize::SmallInput)
        });
    }

    group.finish();
}

// =====================================================================
// detect_regions sub-phases
// =====================================================================

/// Benchmark candidate_set AHashSet construction (BFS setup).
fn bench_bfs_setup(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_regions/bfs_setup");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let store = fixtures::build_store(gen());
        let candidates = reduce_candidate_space(&store);

        group.bench_with_input(BenchmarkId::new("cells", name), &candidates, |b, cands| {
            b.iter(|| {
                let _set: AHashSet<u32> = cands.iter().copied().collect();
            })
        });
    }

    group.finish();
}

/// Benchmark query_radius calls in isolation.
/// This isolates the 49-lookup-per-call cost.
fn bench_query_radius_calls(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_regions/query_radius");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let store = fixtures::build_store(gen());
        let candidates = reduce_candidate_space(&store);

        // Collect (row, col) pairs for candidate cells
        let coords: Vec<(u32, u32)> = candidates
            .iter()
            .map(|&id| (store.get_row(id as usize), store.get_col(id as usize)))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &(store, coords),
            |b, (store, coords)| {
                b.iter(|| {
                    let mut total = 0u32;
                    for &(row, col) in coords {
                        let neighbors = query_radius(store, row, col, NEIGHBOR_RADIUS);
                        total += neighbors.len() as u32;
                    }
                    total
                })
            },
        );
    }

    group.finish();
}

/// Benchmark the neighbor membership check (AHashSet contains + insert).
fn bench_neighbor_membership(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_regions/membership");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let store = fixtures::build_store(gen());
        let candidates = reduce_candidate_space(&store);

        // Pre-collect all neighbor IDs returned by query_radius for each candidate
        let all_neighbors: Vec<Vec<u32>> = candidates
            .iter()
            .map(|&id| {
                let row = store.get_row(id as usize);
                let col = store.get_col(id as usize);
                query_radius(&store, row, col, NEIGHBOR_RADIUS)
            })
            .collect();

        let input = (candidates.clone(), all_neighbors);

        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &input,
            |b, (cands, all_neighbors)| {
                b.iter(|| {
                    let candidate_set: AHashSet<u32> = cands.iter().copied().collect();
                    let mut visited: AHashSet<u32> = AHashSet::with_capacity(cands.len());
                    let mut hits = 0u32;
                    for (i, &cand) in cands.iter().enumerate() {
                        visited.insert(cand);
                        for &neighbor in &all_neighbors[i] {
                            if candidate_set.contains(&neighbor) && visited.insert(neighbor) {
                                hits += 1;
                            }
                        }
                    }
                    hits
                })
            },
        );
    }

    group.finish();
}

/// Benchmark region Vec construction (building Region structs).
fn bench_region_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_regions/region_build");

    for (name, gen) in [
        ("tiny", fixtures::tiny_cells as fn() -> _),
        ("typical", fixtures::typical_cells),
        ("large", fixtures::large_cells),
    ] {
        let mut store = fixtures::build_store(gen());
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);

        // Pre-run detect_regions to get region structure, then benchmark
        // just the Vec<Region> assembly cost
        let region_data: Vec<(usize, Vec<usize>)> = {
            let mut s2 = store.clone();
            let regions = detect_regions(&mut s2, &candidates);
            regions
                .iter()
                .map(|r| (r.region_id, r.cell_ids.clone()))
                .collect()
        };

        group.bench_with_input(
            BenchmarkId::new("cells", name),
            &(store, region_data),
            |b, (store, region_data)| {
                b.iter(|| {
                    let mut regions = Vec::with_capacity(region_data.len());
                    for (region_id, cell_ids) in region_data {
                        let mut header_ids = Vec::new();
                        let mut value_ids = Vec::new();
                        for &id in cell_ids {
                            match store.get_cell_type(id) {
                                gridmap_core::types::CellType::Header => header_ids.push(id),
                                gridmap_core::types::CellType::Value => value_ids.push(id),
                                _ => {}
                            }
                        }
                        regions.push((*region_id, cell_ids.clone(), header_ids, value_ids));
                    }
                    regions
                })
            },
        );
    }

    group.finish();
}

/// Full detect_regions benchmark for baseline comparison.
fn bench_detect_regions_full(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_regions/full");

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

criterion_group!(
    benches,
    // from_raw sub-phases
    bench_raw_collect_and_dedup,
    bench_raw_to_arrow_arrays,
    bench_coord_index_build,
    bench_workspace_alloc,
    bench_comment_merge_path,
    bench_from_raw_full,
    // detect_regions sub-phases
    bench_bfs_setup,
    bench_query_radius_calls,
    bench_neighbor_membership,
    bench_region_construction,
    bench_detect_regions_full,
);
criterion_main!(benches);
