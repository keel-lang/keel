# Installation

## From source (recommended)

Keel is built in Rust. You need the Rust toolchain installed.

```bash
# Clone the repository
git clone https://github.com/keel-lang/keel.git
cd keel

# Build the release binary
cargo build --release

# The binary is at target/release/keel
./target/release/keel --version
```

Optionally, add it to your PATH:

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
  run    Execute an Keel program
  check  Type-check an Keel program without executing
  init   Scaffold a new Keel project
  repl   Interactive REPL
  fmt    Format an Keel file
  build  Compile an Keel file to bytecode
  help   Print this message or the help of the given subcommand(s)
```

## LLM setup

Keel agents use LLMs for AI primitives (`classify`, `draft`, `summarize`, etc.). You need at least one provider:

### Option A: Ollama (local, free)

```bash
# Install Ollama (https://ollama.com)
ollama pull gemma4

# Tell Keel which model to use
export KEEL_OLLAMA_MODEL=gemma4
```

### Option B: Anthropic Claude API

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

See [LLM Providers](../config/llm-providers.md) for detailed configuration.

## Editor support

Install the VS Code extension for syntax highlighting:

```bash
# From the project directory
cd editors/vscode
# Install via VS Code's extension manager, or:
code --install-extension keel-lang-0.1.0.vsix
```

## Next steps

- [Hello World →](./hello-world.md)
- [Your First Agent →](./first-agent.md)
