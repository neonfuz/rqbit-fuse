# LLM Loop Rules

- Only do a single checkbox from the todo checklist
- Never ask for confirmation from the user, this is non interactive. Just do a single item, no more no less
- If you do research for a future item, write your research to the ./research directory, then edit the todo checklist with a reference to that research file
- When you are done with the item, run git diff and validate that you completed the task. Fix problems until you are satisfied with the code quality
- After you have validated the quality of the code, check the item off the todo checklist
- After you check an item off the checklist, append what you have done to CHANGELOG.md
- After you have updated CHANGELOG.md, write a commit message and commit your code to git
- After you have committed to git, if all todo items are done create an empty `.done` file



- Add new todo items if more work is needed

- After completing work, write a git commit message and commit your work
- Update `CHANGELOG.md` after completing tasks
- Only do the first unchecked subtask under parent tasks
