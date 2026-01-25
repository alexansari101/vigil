//! backutil-daemon: Background service for automated backups.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("backutil-daemon starting...");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
