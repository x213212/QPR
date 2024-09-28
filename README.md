Setting up OpenAI API Key for Folder Analysis Project
To configure your OpenAI API key and run the folder analysis project, follow the steps below.

Environment Setup
# Set the OpenAI API Key in the .env file
```
root@DESKTOP-4N2JHAU ~/a/analysisproj (master)# tree -L 2 -a
.
├── .env
```
.env File
```
OPENAI_API_KEY=sk-*************
Custom Configuration
In your Rust project, configure the following constants for folder analysis and code summary generation:
```
main.rs
```rust
// Server port configuration
const SERVER_PORT: u16 = 3030;

// List of code file extensions to filter
const CODE_FILE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "java", "cpp", "c", "go", "sh", "rb", "bat", "cs", "resx", "h", "md",
];

// GPT prompt for folder analysis (includes placeholders {})
const FOLDER_ANALYSIS_PROMPT: &str = "Based on the following folder names, identify potential source code directories written by the user. Return a JSON structure with the key 'analysis_key' and a list of directories that match the criteria:\n{folders}\n{extra_folders}";

// GPT prompt for code summarization
const FILE_SUMMARY_PROMPT: &str = "Generate a concise summary for the following code (no more than 100 words). Use professional software engineering terminology and retain the original variable names for easy analysis. Please describe in Traditional Chinese:\n{}";

// Project directory path
const PROJECT_PATH: &str = "/root/Ghost";
Running the Project
To execute the project, use the following command:
```

```bash
cargo run 
```
This will start the folder analysis process, leveraging the OpenAI API for generating summaries and insights.

Demo Output
Here are example outputs from running the analysis:


![image](https://github.com/user-attachments/assets/bf3f2433-0743-486c-a3ba-42d738fcd0cb)
![image](https://github.com/user-attachments/assets/f6018cf8-442d-4495-b7d1-96e7d4bfceb4)

# demo
![image](https://github.com/user-attachments/assets/f6fa25f6-dedc-4e5f-b050-7810bd29af4b)
![image](https://github.com/user-attachments/assets/14f93f15-af8c-49d5-b602-e0e171950e77)



By following these instructions, you can run the project and analyze folder structures with automated code summaries.
