//! backutil: CLI and TUI client for backutil-daemon.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("backutil CLI starting...");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
