$TB = @{ 'int' = 0x01; 'float' = 0x02; 'bytes' = 0x03 }
$GB = @{ 'pure' = 0x01; 'no_alloc' = 0x02; 'writes_output' = 0x03 }

function F($name,$interp,$size){ [pscustomobject]@{ name=$name; interp=$interp; size=$size } }

function Compute-Hash {
    param($Inputs,$Outputs,$Requires,$Guarantees)
    $bytes = New-Object System.Collections.Generic.List[byte]
    function AddStr($s) { foreach ($b in [System.Text.Encoding]::UTF8.GetBytes($s)) { $bytes.Add($b) } }
    function AddByte($b) { $bytes.Add([byte]$b) }
    function AddLE8($n) { foreach ($b in [System.BitConverter]::GetBytes([int64]$n)) { $bytes.Add($b) } }
    AddStr 'INPUTS:'
    foreach ($i in $Inputs)  { AddStr $i.name; AddByte $TB[$i.interp]; AddLE8 $i.size }
    AddStr 'OUTPUTS:'
    foreach ($o in $Outputs) { AddStr $o.name; AddByte $TB[$o.interp]; AddLE8 $o.size }
    AddStr 'REQUIRES:'
    foreach ($r in $Requires){ AddStr $r[0]; AddStr '@'; AddStr $r[1] }
    AddStr 'GUARANTEES:'
    foreach ($g in $Guarantees){ AddByte $GB[$g] }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $digest = $sha.ComputeHash($bytes.ToArray())
    return (($digest[0..3] | ForEach-Object { $_.ToString('x2') }) -join '')
}

$specs = @(
  @{ n='addr-server';     i=@();                          o=@((F 'addr' 'bytes' 16),(F 'alen' 'int' 8));                              g=@('writes_output') },
  @{ n='addr-client';     i=@();                          o=@((F 'addr' 'bytes' 16),(F 'alen' 'int' 8));                              g=@('writes_output') },
  @{ n='path-input';      i=@();                          o=@((F 's' 'bytes' 16),(F 'n' 'int' 8));                                    g=@('writes_output') },
  @{ n='path-output';     i=@();                          o=@((F 's' 'bytes' 16),(F 'n' 'int' 8));                                    g=@('writes_output') },
  @{ n='extract-body';    i=@((F 'req' 'bytes' 65536),(F 'rlen' 'int' 8)); o=@((F 'body' 'bytes' 65536),(F 'blen' 'int' 8));         g=@('writes_output') },
  @{ n='assemble-output'; i=@((F 'formatted' 'bytes' 32),(F 'fmtlen' 'int' 8),(F 'body' 'bytes' 65536),(F 'bodylen' 'int' 8)); o=@((F 'result' 'bytes' 65536),(F 'total' 'int' 8)); g=@('writes_output') },
  @{ n='make-request';    i=@((F 'body' 'bytes' 65536),(F 'blen' 'int' 8)); o=@((F 'req' 'bytes' 65536),(F 'reqlen' 'int' 8));       g=@('writes_output') }
)

$lines = foreach ($s in $specs) {
  $h = Compute-Hash -Inputs $s.i -Outputs $s.o -Requires @() -Guarantees $s.g
  "{0,-18} {1}" -f $s.n, $h
}
$lines | Set-Content -Encoding ascii "<repo-root>\experiments\http-relay/run-2\trials\sigil-noex-t3\hashes.txt"
$lines | ForEach-Object { Write-Host $_ }
