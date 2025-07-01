# Roblox Studio Gemini Connector

This repository provides a server that connects Roblox Studio to Google's Gemini API, allowing you to use natural language prompts to modify your Roblox place and run code.

It consists of two main parts:
- A Rust-based web server that connects to the Gemini API and manages a queue of tasks.
- A Roblox Studio plugin that polls the server for tasks (like running code or inserting a model) and executes them.

**Warning**: This server connects to a third-party service (Google's Gemini API) and allows it to execute code within your Roblox Studio environment. Review Google's privacy policies and use with caution.

## How to Use

### 1. Install the Plugin
First, you need to build the project and install the companion plugin for Roblox Studio.

- [Install Rust](https://www.rust-lang.org/tools/install) if you haven't already.
- Clone this repository.
- Run the following command from the root of the repository:
  ```sh
  cargo run