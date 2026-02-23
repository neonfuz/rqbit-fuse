# LLM Loop Rules

- Only do a single checkbox from the todo checklist.
- If all todo items are done in the entire todo checklist, then just create an empty `.done` file
- Never ask for confirmation from the user, this is non interactive. Just do a single item, no more no less
- If you do research for a future item, write your research to the ./research directory, then edit the todo checklist with a reference to that research file
- When you are done with the item, run git diff and validate that you completed the task. Fix problems until you are satisfied with the code quality
- After you have validated the quality of the code, check the item off the todo checklist
- After you check an item off the checklist, append what you have done to CHANGELOG.md
- After you have updated CHANGELOG.md, write a commit message and commit your code to git
- If you are working on a task and it is very large or you are having a hard time completing it, break the task down into more TODO items and append them to the todo checklist
