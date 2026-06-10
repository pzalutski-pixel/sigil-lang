param(
  [Parameter(Mandatory=$true)][string]$Spec
)
# Spec format: lines like:
#   IN <name> <int|float|bytes> <size>
#   OUT <name> <int|float|bytes> <size>
#   REQ <name> <hash>
#   GUA <pure|no_alloc|writes_output>
$bytes = New-Object System.Collections.Generic.List[byte]
function AddStr([string]$s){ foreach($b in [System.Text.Encoding]::UTF8.GetBytes($s)){ $bytes.Add($b) } }
function AddLE8([long]$v){ $b=[BitConverter]::GetBytes([int64]$v); foreach($x in $b){ $bytes.Add($x) } }
function TypeByte([string]$t){ switch($t){ 'int'{0x01} 'float'{0x02} 'bytes'{0x03} } }
function GuaByte([string]$g){ switch($g){ 'pure'{0x01} 'no_alloc'{0x02} 'writes_output'{0x03} } }

$lines = $Spec -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' }
$inputs=@(); $outputs=@(); $reqs=@(); $guas=@()
foreach($l in $lines){
  $p = $l -split '\s+'
  switch($p[0]){
    'IN'  { $inputs  += ,@($p[1],$p[2],$p[3]) }
    'OUT' { $outputs += ,@($p[1],$p[2],$p[3]) }
    'REQ' { $reqs    += ,@($p[1],$p[2]) }
    'GUA' { $guas    += $p[1] }
  }
}
AddStr "INPUTS:"
foreach($i in $inputs){ AddStr $i[0]; $bytes.Add([byte](TypeByte $i[1])); AddLE8 ([long]$i[2]) }
AddStr "OUTPUTS:"
foreach($o in $outputs){ AddStr $o[0]; $bytes.Add([byte](TypeByte $o[1])); AddLE8 ([long]$o[2]) }
AddStr "REQUIRES:"
foreach($r in $reqs){ AddStr $r[0]; AddStr "@"; AddStr $r[1] }
AddStr "GUARANTEES:"
foreach($g in $guas){ $bytes.Add([byte](GuaByte $g)) }

$sha = [System.Security.Cryptography.SHA256]::Create()
$digest = $sha.ComputeHash($bytes.ToArray())
$hex = ($digest[0..3] | ForEach-Object { $_.ToString('x2') }) -join ''
Write-Output $hex
