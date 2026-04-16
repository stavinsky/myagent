---
description: "Review a file for errors, AI slop comments, and style issues, then fix them systematically"
argument-hint: "File path to review and fix"
agent: "agent"
tools: [read_file, edit_file, grep]
---

## Task: Review and Fix File Issues

### Step 1: Review the File
Read the entire file and analyze it for:
1. **Errors** - Syntax errors, type mismatches, missing imports, incorrect logic
2. **AI slop comments** - Unnecessary filler phrases like "Here's the code you requested", "Certainly!", "I apologize for..." or other low-value AI-generated commentary
3. **Style issues** - Inconsistent formatting, poor naming, missing documentation, code smells

### Step 2: Create Todo List
Create a structured todo list with all identified issues:
```
## Issues Found

### Errors
1. [ERROR] <file>:<line> - <description>
   - **Issue**: <what's wrong>
   - **Fix**: <concrete recommendation>

### AI Slop
2. [SLOP] <file>:<line> - <description>
   - **Issue**: <unnecessary filler comment>
   - **Fix**: <remove or improve>

### Style
3. [STYLE] <file>:<line> - <description>
   - **Issue**: <style issue>
   - **Fix**: <recommendation>
```

### Step 3: Fix Issues One by One
For each issue in priority order (Errors > AI Slop > Style):
1. Read the relevant section of the file
2. Apply the fix using `edit_file`
3. Verify the fix by reading the file again
4. Mark the issue as resolved

### Priority Rules
- **Errors** first - critical bugs, security issues, data-loss risks
- **AI Slop** second - remove unnecessary comments that don't add value
- **Style** last - only fix if there are no more important issues

### Output Format
After each fix, output:
```
### Fixed: [<issue-type>] <file>:<line>
✅ Applied fix for: <short description>
```

### Final Summary
When all fixes are complete, output:
```
## Summary
- Errors fixed: <count>
- AI slop removed: <count>  
- Style issues addressed: <count>
- Remaining issues: <count> (if any)
```