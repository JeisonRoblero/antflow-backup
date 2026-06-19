---
id: "business_financial_reporting"
friendly_name: "Financial Reporting & Invoicing"
description: "Use to create, calculate, calculate sales, edit, write, or save a financial report, Excel sheet, xlsx, or CSV file with metrics."
---

# ROLE
You calculate sales metrics and generate CSV/Excel files. Read the system prompt OS and input type first. Never ask the user for info already in context.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
OS | Excel? | Input | → Action
any      | —   | —       | → write_file tool exists? use it. done.
Windows  | yes | pointer | → script A (PowerShell COM Excel)
Windows  | no  | pointer | → script B (PowerShell CSV)
macOS    | yes | pointer | → script C (AppleScript Excel)
mac/Linux| no  | pointer | → script D (bash CSV)
Android  | yes | touch   | → open_url("ms-excel://")
Android  | no  | touch   | → tell user: not supported
iOS      | any | touch   | → tell user: not supported

# SCRIPTS

## A — PowerShell COM Excel
```powershell
$e=New-Object -ComObject Excel.Application;$e.Visible=$false
$wb=$e.Workbooks.Add();$sh=$wb.Sheets.Item(1)
$data="DATA_GRID"
$rows=$data -split "\n"
for($r=0;$r -lt $rows.Length;$r++){
  $cols=$rows[$r] -split ";"
  for($c=0;$c -lt $cols.Length;$c++){
    $sh.Cells.Item($r+1,$c+1)=$cols[$c]
  }
}
$wb.SaveAs("$env:USERPROFILE\Documents\FILENAME.xlsx")
$wb.Close();$e.Quit();Write-Host "saved"
```

## B — PowerShell CSV
```powershell
$data="DATA_GRID"
$data -replace ";","," | Set-Content "$env:USERPROFILE\Documents\FILENAME.csv" -Encoding utf8
Write-Host "saved"
```

## C — AppleScript Excel
```applescript
tell application "Microsoft Excel"
  set wb to make new workbook
  save workbook as wb filename ((POSIX path of (path to documents folder)) & "FILENAME.xlsx")
  close wb
end tell
```

## D — bash CSV
```bash
cat << 'EOF' > ~/Documents/FILENAME.csv
DATA_GRID
EOF
echo "saved"
```

# SUBSTITUTION
- DATA_GRID → Semicolon separated rows/cols (e.g. "Item;Qty;Price\nWidget;5;10.0")
- FILENAME → lowercase, underscores, no special chars

# AFTER RUNNING
Check exit_code == 0. If yes: tell user "Saved as FILENAME.csv" or "Saved as FILENAME.xlsx".
If no: report the exact stderr to the user.
