# Radar-Lend

Radar-Lend is a Solana program that provides two main functionalities:
1. Deposit and withdraw SOL
2. USDC loans with SOL collateral

## Features

- **Deposit and Withdraw SOL:**
  - Users can deposit SOL into the program
  - Users can withdraw their deposited SOL

- **USDC Loans with SOL Collateral:**
  - Users can take USDC loans by providing SOL as collateral
  - 25% Loan-to-Value (LTV) ratio
  - Hard-coded SOL price 
  - Loan tracking system using Program Derived Addresses (PDAs)

## Prerequisites

Before you begin, ensure you have the following installed:
- [Rust](https://www.rust-lang.org/tools/install)
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools)
- [Node.js and npm](https://nodejs.org/en/download/) (for client-side interactions, if applicable)

## Setup

1. Clone the repository:
   ```
   git clone https://github.com/ahmadkhan100/radar-lend.git
   cd radar-lend
   ```

2. Build the project:
   ```
   cargo build-bpf
   ```

## Project Structure

- `src/main_deposit_withdraw.rs`: Contains the logic for SOL deposits and withdrawals
- `src/main_usdc_sol_collateral.rs`: Contains the logic for USDC loans with SOL collateral
- `tests/`: Contains test files for both functionalities

## Running Tests

To run all tests for the Radar-Lend project:

1. Ensure you're in the project root directory.

2. Run the following command:
   ```
   cargo test-bpf
   ```

To run specific tests:

- For deposit/withdraw functionality:
  ```
  cargo test-bpf test_deposit
  cargo test-bpf test_withdraw
  ```

- For USDC loan functionality:
  ```
  cargo test-bpf test_initialize_loan
  cargo test-bpf test_repay_loan
  cargo test-bpf test_liquidate_loan
  ```

## Deployment

To deploy the Radar-Lend program to Solana:

1. Build the program:
   ```
   cargo build-bpf
   ```

2. Deploy to Solana:
   ```
   solana program deploy target/deploy/radar_lend.so
   ```

Make note of the program ID after deployment, as you'll need it for client interactions.

## Usage

### Deposit and Withdraw SOL

(Provide brief instructions or code snippets for how to interact with the deposit/withdraw functionality, either using CLI commands or client-side code)

### USDC Loans with SOL Collateral

(Provide brief instructions or code snippets for how to:
1. Initialize a loan
2. Repay a loan
3. Liquidate a loan (if applicable)

You can use CLI commands or client-side code examples)

## Configuration

- SOL price is currently hard-coded at $150 (can be found in `src/main_usdc_sol_collateral.rs`)
- LTV ratio is set to 25% (can be found in `src/main_usdc_sol_collateral.rs`)
- USDC mint address and program USDC account need to be set before deployment (in `src/main_usdc_sol_collateral.rs`)

## Contributing

If you'd like to contribute to the project, please follow these steps:
1. Fork the repository
2. Create a new branch for your feature
3. Commit your changes
4. Push to your branch
5. Create a new Pull Request

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
