use std::{fmt::Write, hint};

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use indoc::indoc;
use nagato_apply::{apply, Parser};

fn generate_large_data(
  lines_count: usize,
  hunk_interval: usize,
) -> (String, Vec<u8>) {
  let mut source = String::with_capacity(lines_count * 30);
  for i in 0..lines_count {
    writeln!(source, "this is line number {}", i).unwrap();
  }

  let mut patch = String::new();
  patch.push_str("diff --git a/large.txt b/large.txt\n");
  patch.push_str("index 0000000..1111111 100644\n");
  patch.push_str("--- a/large.txt\n");
  patch.push_str("+++ b/large.txt\n");

  for i in (0..lines_count).step_by(hunk_interval) {
    let line_num = i + 1;
    writeln!(patch, "@@ -{},1 +{},1 @@", line_num, line_num).unwrap();
    writeln!(patch, "-this is line number {}", i).unwrap();
    writeln!(patch, "+this is CHANGED line number {}", i).unwrap();
  }

  (patch, source.into_bytes())
}

fn bench_apply(c: &mut Criterion) {
  let small_diff = indoc! {r#"
    diff --git a/file.txt b/file.txt
    index 1234567..abcdefg 100644
    --- a/file.txt
    +++ b/file.txt
    @@ -1,5 +1,5 @@
     line 1
     line 2
    -line 3
    +new line 3
     line 4
     line 5
  "#};
  let small_source = b"line 1\nline 2\nline 3\nline 4\nline 5\n";

  let mut group = c.benchmark_group("small_file");

  group.bench_function("parse", |b| {
    b.iter(|| {
      let parser = Parser::new(hint::black_box(small_diff.as_bytes()));
      for patch in parser {
        let _ = patch.unwrap();
      }
    })
  });

  group.bench_function("apply", |b| {
    let patch = Parser::new(small_diff.as_bytes()).next().unwrap().unwrap();
    b.iter(|| {
      let mut output = Vec::new();
      apply(
        &mut output,
        hint::black_box(&patch),
        hint::black_box(small_source.as_slice()),
      )
      .unwrap();
    })
  });
  group.finish();

  let (large_diff, large_source) = generate_large_data(10_000, 100);

  let mut group = c.benchmark_group("large_file_10k");
  group.throughput(Throughput::Bytes(large_source.len() as u64));

  group.bench_function("parse", |b| {
    b.iter(|| {
      let parser = Parser::new(hint::black_box(large_diff.as_bytes()));
      for patch in parser {
        let _ = patch.unwrap();
      }
    })
  });

  group.bench_function("apply", |b| {
    let patch = Parser::new(large_diff.as_bytes()).next().unwrap().unwrap();
    b.iter(|| {
      let mut output = Vec::with_capacity(large_source.len() + 1024);
      apply(
        &mut output,
        hint::black_box(&patch),
        hint::black_box(large_source.as_slice()),
      )
      .unwrap();
    })
  });
  group.finish();
}

criterion_group!(benches, bench_apply);
criterion_main!(benches);
