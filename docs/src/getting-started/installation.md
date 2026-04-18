# Installation

> **Alpha (v0.1).** No binary release yet. Build from source.

## Build from source

Keel is built in Rust. You need the Rust toolchain installed (`rustup`).

```bash
git clone https://github.com/keel-lang/keel.git
cd keel
cargo build --release
./target/release/keel --version
```

Optionally put the binary on your PATH:

```bash
cp target/release/keel /usr/local/bin/
```

## Verify installation

```bash
keel --help
```

You should see:

```
Keel — AI agents as first-class citizens

Usage: keel <COMMAND>

Commands:
  run    Execute a Keel program
  check  Type-check a Keel program without executing
  init   Scaffold a new Keel project
  repl   Interactive REPL
  fmt    Format a Keel file
  build  Compile a Keel file to bytecode
  help   Print this message or the help of the given subcommand(s)
```

## LLM setup

Keel's `Ai.*` functions call a local Ollama instance. In v0.1 it is the only supported backend.

```bash
# Install Ollama from https://ollama.com
ollama pull gemma4

# Tell Keel which model to use
export KEEL_OLLAMA_MODEL=gemma4
```

See [LLM Providers](../config/llm-providers.md) and [Ollama Setup](../config/ollama.md) for details.

## Editor support

VS Code extension (syntax highlighting + LSP):

```bash
cd editors/vscode
code --install-extension .
```

## What's not available yet

- **Homebrew tap** — returns with the first v0.1.x binary release
- **One-line installer** (`curl | sh`) — same
- **Pre-built binaries** on GitHub Releases — same

All three come back once v0.1.0 is cut as a binary release.

## Next steps

- [Hello World →](./hello-world.md)
- [Your First Agent →](./first-agent.md)
