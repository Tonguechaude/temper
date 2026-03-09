mod check_chunks;

pub fn validate() -> Result<(), String> {
    let state = temper_state::create_state(std::time::Instant::now());
    check_chunks::check_chunks(&state)?;

    println!("Validation completed successfully. All chunks are valid.");

    Ok(())
}
