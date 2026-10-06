use criterion::{criterion_group, criterion_main, Criterion};

use gridmap_core::pipeline::{process_sheet, process_workbook};
use gridmap_core::store::RawCell;

fn make_cells(n: usize) -> Vec<RawCell> {
    let cols = 10;
    (0..n)
        .map(|i| {
            let row = (i / cols) as u32;
            let col = (i % cols) as u32;
            RawCell {
                row,
                col,
                value: format!("data_{row}_{col}"),
                formula: String::new(),
                comment: String::new(),
                sheet_name: "Sheet1".into(),
                is_merged_origin: false,
            }
        })
        .collect()
}

fn make_cells_with_credentials(n: usize) -> Vec<RawCell> {
    let mut cells = make_cells(n);
    // Inject a password header + value pair
    if cells.len() >= 2 {
        cells[0].value = "Password".into();
        cells[1].value = "s3cRet!99".into();
    }
    cells
}

fn bench_from_raw(c: &mut Criterion) {
    let cells_10k = make_cells(10_000);
    c.bench_function("from_raw_10k", |b| {
        b.iter(|| {
            let _ = gridmap_core::store::CellStore::from_raw(cells_10k.clone());
        })
    });

    let cells_100k = make_cells(100_000);
    c.bench_function("from_raw_100k", |b| {
        b.iter(|| {
            let _ = gridmap_core::store::CellStore::from_raw(cells_100k.clone());
        })
    });
}

fn bench_process_sheet(c: &mut Criterion) {
    let cells_1k = make_cells_with_credentials(1_000);
    c.bench_function("process_sheet_1k", |b| {
        b.iter(|| {
            let _ = process_sheet(cells_1k.clone());
        })
    });

    let cells_5k = make_cells_with_credentials(5_000);
    c.bench_function("process_sheet_5k", |b| {
        b.iter(|| {
            let _ = process_sheet(cells_5k.clone());
        })
    });
}

fn bench_process_workbook(c: &mut Criterion) {
    let sheets: Vec<Vec<RawCell>> = (0..10)
        .map(|_| make_cells_with_credentials(5_000))
        .collect();
    c.bench_function("process_workbook_10x5k", |b| {
        b.iter(|| {
            let _ = process_workbook(sheets.clone());
        })
    });
}

criterion_group!(
    benches,
    bench_from_raw,
    bench_process_sheet,
    bench_process_workbook
);
criterion_main!(benches);
