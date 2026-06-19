---
id: "office_word_document_creation"
friendly_name: "Word Document Creation"
description: "Use when user wants to write, create, edit, open, type, or save a Word document or docx file."
---

# ROLE
You create Word (.docx) files. Read the system prompt OS and input type first. Never ask the user for info already in context.

# DECISION TABLE — pick ONE row, execute it
# FORMAT: 
OS | Word? | Input | → Action
any      | —   | —       | → write_file tool exists? use it. done.
Windows  | yes | pointer | → script A (PowerShell COM)
Windows  | no  | pointer | → script B (PowerShell OOXML)
macOS    | yes | pointer | → script C (AppleScript)
mac/Linux| no  | pointer | → script D (bash + zip)
Android  | yes | touch   | → open_url("ms-word://") → tap → type_text
Android  | no  | touch   | → tell user: not supported
iOS      | any | touch   | → tell user: not supported

# SCRIPTS

## A — PowerShell COM
```powershell
$w=New-Object -ComObject Word.Application;$w.Visible=$false
$d=$w.Documents.Add();$d.Content.Text="CONTENT"
$d.SaveAs2("$env:USERPROFILE\Documents\FILENAME.docx",16)
$d.Close();$w.Quit();Write-Host "saved"
```

## B — PowerShell OOXML (no Word needed)
```powershell
Add-Type -Assembly System.IO.Compression.FileSystem
$t="$env:TEMP\dx";New-Item -Force -ItemType Directory "$t\word","$t\_rels","$t\word\_rels"|Out-Null
Set-Content "$t\word\document.xml" -Encoding utf8 '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>CONTENT</w:t></w:r></w:p></w:body></w:document>'
Set-Content "$t\_rels\.rels" -Encoding utf8 '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'
Set-Content "$t\word\_rels\document.xml.rels" -Encoding utf8 '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>'
Set-Content "$t\[Content_Types].xml" -Encoding utf8 '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'
[IO.Compression.ZipFile]::CreateFromDirectory($t,"$env:USERPROFILE\Documents\FILENAME.docx")
Remove-Item $t -Recurse -Force;Write-Host "saved"
```

## C — AppleScript
```applescript
tell application "Microsoft Word"
  set d to make new document
  set content of text object of d to "CONTENT"
  save as d file name ((POSIX path of (path to documents folder)) & "FILENAME.docx")
  close d
end tell
```

## D — bash + zip
```bash
T=$(mktemp -d);mkdir -p "$T/word" "$T/_rels" "$T/word/_rels"
printf '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>CONTENT</w:t></w:r></w:p></w:body></w:document>' > "$T/word/document.xml"
printf '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>' > "$T/_rels/.rels"
printf '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>' > "$T/word/_rels/document.xml.rels"
printf '<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>' > "$T/[Content_Types].xml"
cd "$T"&&zip -rq ~/Documents/FILENAME.docx .;rm -rf "$T";echo "saved"
```

# SUBSTITUTION
- CONTENT → user's text (escape XML: & → &amp;  < → &lt;)
- FILENAME → lowercase, underscores, no special chars

# AFTER RUNNING
Check exit_code == 0. If yes: tell user "Saved as FILENAME.docx".
If no: report the exact stderr to the user.
