# BOTMONAD Project Instructions

## Project Overview
BOTMONAD is a hybrid project containing:
1. A Node.js wrapper for Google's Gemini CLI (`@google/gemini-cli`).
2. A Rust-based high-frequency trading bot for the Monad blockchain (`monad-bot`).

## Architecture

### Node.js Component
- **Entry Point**: Uses `@google/gemini-cli`.
- **Purpose**: CLI interactions and agent orchestration.

### Rust Component (`monad-bot`)
- **Purpose**: Low-latency trading bot using Monad Execution Events.
- **Dependencies**: `monad-exec-events`, `monad-event-ring` (from `category-labs/monad-bft`).
- **Build Requirements**: Clang 19+, CMake, Rust 1.75+.

## Configuration
Agent instructions are synchronized across multiple files:
- `.github/copilot-instructions.md` (GitHub Copilot)
- `AGENTS.md` (general agents)
- `CLAUDE.md` (Claude)
- `.clinerules/byterover-rules.md` (Cline)
- `.kilocode/rules/byterover-rules.md` (Kilocode)

## Developer Workflow

### Node.js
```bash
npm install
export GEMINI_API_KEY=your_key_here
```

### Rust (`monad-bot`)
```bash
cd monad-bot
# Install system dependencies (Ubuntu/Debian)
sudo apt install clang-19 libzstd-dev libhugetlbfs-dev cmake

# Ensure Clang 19+ is used
export CC=clang-19
cargo build
```

## Conventions
- Keep agent instruction files in sync when updating rules.
- Use `npm` for Node.js package management.
- Use `cargo` for Rust package management.

---

## Byterover MCP Tools (Required)

### 1. `byterover-store-knowledge`
You **MUST** always use this tool when:
- Learning new patterns, APIs, or architectural decisions from the codebase
- Encountering error solutions or debugging techniques
- Finding reusable code patterns or utility functions
- Completing any significant task or plan implementation

### 2. `byterover-retrieve-knowledge`
You **MUST** always use this tool when:
- Starting any new task or implementation to gather relevant context
- Before making architectural decisions to understand existing patterns
- When debugging issues to check for previous solutions
- Working with unfamiliar parts of the codebase

[byterover-mcp]

[byterover-mcp]

You are given two tools from Byterover MCP server, including
## 1. `byterover-store-knowledge`
You `MUST` always use this tool when:

+ Learning new patterns, APIs, or architectural decisions from the codebase
+ Encountering error solutions or debugging techniques
+ Finding reusable code patterns or utility functions
+ Completing any significant task or plan implementation

## 2. `byterover-retrieve-knowledge`
You `MUST` always use this tool when:

+ Starting any new task or implementation to gather relevant context
+ Before making architectural decisions to understand existing patterns
+ When debugging issues to check for previous solutions
+ Working with unfamiliar parts of the codebase
