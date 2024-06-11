use clap::Parser;
use sc_cli::{
    GenerateCmd, GenerateKeyCmdCommon, InspectKeyCmd, InspectNodeKeyCmd, SignCmd, VanityCmd,
    VerifyCmd,
};

/// Wallet sub-commands.
#[derive(Debug, Parser)]
#[command(
    name = "wallet",
    about = "Utility for generating and restoring keys",
    version
)]
pub enum WalletCmd {
    /// Generate a random node key, write it to a file or stdout and write the
    /// corresponding peer-id to stderr
    GenerateNodeKey(GenerateKeyCmdCommon),

    /// Generate a random account
    Generate(GenerateCmd),

    /// Gets a public key and a SS58 address from the provided Secret URI
    Inspect(InspectKeyCmd),

    /// Load a node key from a file or stdin and print the corresponding peer-id
    InspectNodeKey(InspectNodeKeyCmd),

    /// Sign a message, with a given (secret) key.
    Sign(SignCmd),

    /// Generate a seed that provides a vanity address.
    Vanity(VanityCmd),

    /// Verify a signature for a message, provided on STDIN, with a given (public or secret) key.
    Verify(VerifyCmd),
}
