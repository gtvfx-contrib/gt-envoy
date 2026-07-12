fn main() {
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    std::process::exit(envoy_cli::run(&argv));
}
