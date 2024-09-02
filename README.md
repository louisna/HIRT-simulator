# HIRT simulator

This repository is a simulator of HIRT written in Rust.
Its purpose is to compare the performances of HIRT with [Maelstrom](https://d1wqtxts1xzle7.cloudfront.net/51081954/Maelstrom_Transparent_Error_Correction_f20161227-26902-1vkqa2r-libre.pdf?1482876138=&response-content-disposition=inline%3B+filename%3DMaelstrom_Transparent_Error_Correction_f.pdf&Expires=1725284732&Signature=Mdvb5-CBfHQpnM430tK8r9The36KFuXQ8WC-f8yiAcVhP0E8rZ8xM7RF6kUgOdpuBg~OR7zKNVSVhOkR1Og4LL3yoeZk3zpY8GnUdpI-B1TOlJM9YAXoYHOug74htamjm4-2psNpmQzVJ50myw~YXH4v5JwauCHuttfRbvAbc-MpsjfsYBSf4kxU432yMO4QkZtlBCI~Yostd4gF4wod~O~5Zkk9aik1LDQ3EOIA4ejsujkHrFtsxm6lYq3If1c7i1lvmyhOJo2QYOCYcgxAFqzKcCVaQgu6YrCkn~s~7BNPFSv5H2uK9xZO5JgkfBN8bf2AFXIePS-OI1~wdRS6IQ__&Key-Pair-Id=APKAJLOHF5GGSLRBV4ZA).

[The High-speed Robust Tunnel (HIRT)](https://github.com/louisna/HIRT.git) is a high-speed network-layer Forward Erasure Correction tunnel leveraging IPv6 Segment Routing.

Maelstrom is a 2011 project implementing interleaved XOR-based Forward Erasure Correction (FEC) below the transport layer for inter-DC communication. 

Since both project suggest the use of FEC at the network-layer, we build this simulator to evaluate their performance, i.e., HIRT with its adaptive Random Linear Coding (RLC) versus Maelstrom with interleaved XOR.
Since the source code of Maelstrom is not publicly available (at the time of writing this text), we decided to build a simulator to compare both approaches.

## About the simulator

The simulator uses reproducible loss models that should be sufficient for most patterns you'ld like to simulator:
- Uniform
- Gilbert-Eliott

We implemented the whole Maelstorm project in Rust. The mechanism of HIRT is also fully implemented. The RLC library used in this simulator is not public yet, but will be soon.

## Usage

The code is the documentation.

More seriously, I think that the help (e.g., by running `cargo run -- --help`) should display enough information to use the simulator.
As an example, here is the command (using `cargo`) to simulate 10000 source symbols under a uniform drop rate of 2% with HIRT:

```bash
$ cargo run --release -- -n 10000 --set-initial-loss --beta 3 --alpha 0.9 --drop uniform -d results-uniform --u-loss 0.02 -f tart -s 42 --window 200 --rtrace results-uniform-trace
```

This will start HIRT with initial loss estimation of 2% and a seed of 42 (used for reproducible loss patterns).

### Output results

The output results are stored in the `results-uniform` repository according to the previous example.
A file is generated for each run with the input parameters.
For example, with the above example, the output will be stored in `results-uniform/tart_adaptive_0.9_3_200-Uniform-0.02-10000-42.csv`.

The result is a CSV file that looks like this. The results are directly generated with the above command (yeah, the reproducible losses is a banger):

```
n-repair,n-lost,n-recovered,n-ss-drop,n-drop,"ratio,post"
773,0,187,187,200,0.0185649308456326
```

In order:
- The number of generated repair symbols;
- The number of source symbols that were not recovered (hence, lost);
- The number of recovered source symbols;
- The number of source symbols that were dropped by the drop model (this is different from `n-lost` since this value also shows source symbols that were lost but recovered by the FEC algorithm);
- The number of symbols dropped by the drop model (= dropped source and repair symbols);
- The ratio of symbols erased by the drop model, a posteriori. As we can see, this value is slightly below the expected value of 2%, that is why we record it.

## Cite

This simulator is part of the [The High-speed Robust Tunnel (HIRT)](https://github.com/louisna/HIRT.git) project. Please cite this paper if you use the simulator or its results.

TODO: add proper citation.