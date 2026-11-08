## Business Requirements

1. An application which allows the user to sort information on pages into folders into sections into fileboxes.
2. A page can have any combination of text, tables, pictures, links, attachments, etc.
3. Pages should be flexible on how the data is placed and formated.
4. Text on pages can be modified using standard formatting such as itallics, bold, size, justification, color, bullets, block code, etc.
5. Pages, folder, sections and fileboxes need names/titles.
6. File boxes need to be exportable/importable along with attachments.

## Technical Details
1. Fileboxes should be switchable with a dropdown.
2. Sections should be tabs across the top
3. Folders will be tabs along the side
4. Pages will be in a list panel next to the folders tabs.
5. Written in Rust.
6. Provide ability to web deploy on github using github actions from main.
7. Files should store locally or on personal cloud storage such as onedrive or google docs.

## Strategy
1. Write plan with success criteria for each phase to be checked off. Include project scaffolding, including .gitignore, and rigorous unit testing.
2. Do not execute the plan until it has been verified.

## Coding Standards

1. Use latest versions of libraries and idiomatic approaches as of today
2. Keep it simple - NEVER over-engineer, ALWAYS simplify, NO unnecessary defensive programming. No extra features - focus on simplicity.
3. Be concise. Keep README minimal and update as changes are made. IMPORTANT: no emojis ever.
4. During unit testing limit CPU utilization to prevent race condition, overheating and thermal shutdown. 