# Radar-Lend

Radar-Lend is a Solana program that allows users to deposit and withdraw SOL.

## Prerequisites

Before you begin, ensure you have the following installed:
- [Rust](https://www.rust-lang.org/tools/install)
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools)

## Setup

1. Clone the repository:
   ```
   git clone https://github.com/ahmadkhan100/radar-lend.git
   cd radar-lend
   ```

2. Build the project:
   ```
   cargo build
   ```

## Running Tests

To run the tests for the Radar-Lend project, follow these steps:

1. Ensure you're in the project root directory.

2. Run the following command to execute all tests:
   ```
   cargo test
   ```

   This will run all the tests defined in the project, including the deposit and withdraw functionality tests.

3. To run a specific test, you can use the test name. For example:
   ```
   cargo test test_deposit
   ```
   ```
   cargo test test_withdraw
   ```

4. For more verbose output, you can add the `--nocapture` flag:
   ```
   cargo test -- --nocapture
   ```

5. To run tests with logging, you can set the `RUST_LOG` environment variable:
   ```
   RUST_LOG=debug cargo test
   ```

## Test Files

The main test files are located in the `src` directory:
- `main_deposit.rs`: Contains tests for the deposit functionality.
- `main_deposit_withdraw.rs`: Contains tests for both deposit and withdraw functionalities.

## Troubleshooting

If you encounter any issues while running the tests:

1. Ensure all dependencies are up to date:
   ```
   cargo update
   ```

2. Clean and rebuild the project:
   ```
   cargo clean
   cargo build
   ```

3. Check that your Solana CLI is on the correct network and properly configured.


## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
