<p align="center">
  <img src="https://raw.githubusercontent.com/lacodda/kasl/main/kasl.webp" width="320" alt="kasl">
</p>
<h1 align="center">kasl: Key Activity Synchronization and Logging 🕒</h1>
<br>

## Overview 📖

kasl is a comprehensive command-line utility 🛠️ designed to streamline the tracking of work activities 📊, including start times ⏰, pauses ☕, and task completion ✅. It automates the collection of work data 📈, facilitates task management 📋, and generates daily reports 📝, simplifying workflow and productivity tracking 🚀.

## Features 🌟

- **Automatic Data Collection** 📊: Tracks the start of work sessions and pauses without manual input.
- **Task Management** 📋: Easily add tasks and update completion percentages.
- **Daily Reports** 📝: Auto-generates daily reports summarizing work activities.
- **API Integration** 🌐: Sends daily reports to a specified API for easy access and storage.
- **User-Friendly** 😊: Designed with a focus on simplicity and ease of use.

## Getting Started 🚀

### Prerequisites 📋

- Ensure you have a compatible operating system (Windows, macOS, or Linux) 💻.
- Requires Node.js and npm (or an equivalent package manager) for running the utility 📦.

## Installation 🛠️

Install kasl now. kasl is installed by running one of the following commands in your terminal. 
You can install this via the command-line with either curl or wget. 

### Install kasl via curl

```bash
sh -c "$(curl -fsSL https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh)"
```

### Install kasl via wget

```bash
sh -c "$(wget https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh -O -)"
```

This will make the `kasl` application globally available in your system.

### Usage 📚

To start kasl and begin tracking:

```bash
kasl --help
```

## Debugging 🔍

kasl supports enhanced logging for debugging purposes. By default, only clean messages are displayed to users.

### Enable debug logging

To see detailed logs with timestamps, module paths, and debug information:

```bash
# Enable debug mode with full formatting
KASL_DEBUG=1 kasl watch

# Or use standard Rust logging with custom format
RUST_LOG=kasl=debug KASL_LOG_FORMAT=full kasl watch

# For even more verbose output
RUST_LOG=kasl=trace KASL_LOG_FORMAT=full kasl watch
```
### Logging environment variables

- `KASL_DEBUG` - Enables debug level logging with full formatting
- `KASL_LOG_FORMAT=full` - Shows timestamps, module paths, and thread info
- `RUST_LOG` - Standard Rust logging configuration (e.g., kasl=debug, kasl::monitor=trace)

## Roadmap 🗺️

- [ ] Enhance task management with categories and priorities.
- [ ] Integrate with more APIs for report submission.
- [ ] Implement machine learning for predicting task completion time.
- [ ] Add support for team collaboration features.
- [ ] Develop a graphical user interface (GUI) version.

## How to Contribute 🤝

Contributions are welcome! If you have ideas for new features or improvements, feel free to fork the repository, make your changes, and submit a pull request.

## License 📄

kasl is open-source software licensed under the MIT license. See the [LICENSE](LICENSE) file for more details.