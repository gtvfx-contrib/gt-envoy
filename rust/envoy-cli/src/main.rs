mod app;
mod args;

fn main() {
    let exit_code = app::run(None);
    std::process::exit(exit_code);
}
