# Computes Sigil contract hash per the LANGUAGE-REFERENCE HASH section.
# Usage: provide inputs, outputs, requires, guarantees as structured args via here-strings.
# We define a function and then call it with the spec.

function Get-SigilHash {
    param(
        [array]$Inputs,    # each: @{name=...; type=...; size=...}
        [array]$Outputs,   # each: @{name=...; type=...; size=...}
        [array]$Requires,  # each: "name@hash"
        [array]$Guarantees # each: "pure"|"no_alloc"|"writes_output"
    )
    $typeByte = @{ 'int' = 0x01; 'float' = 0x02; 'bytes' = 0x03 }
    $guarByte = @{ 'pure' = 0x01; 'no_alloc' = 0x02; 'writes_output' = 0x03 }
    $ms = New-Object System.IO.MemoryStream
    function Add-Str($s){ $bytes=[System.Text.Encoding]::UTF8.GetBytes($s); $ms.Write($bytes,0,$bytes.Length) }
    function Add-Byte($b){ $ms.WriteByte([byte]$b) }
    function Add-Size($n){ $bytes=[System.BitConverter]::GetBytes([int64]$n); if(-not [System.BitConverter]::IsLittleEndian){[Array]::Reverse($bytes)}; $ms.Write($bytes,0,8) }

    Add-Str "INPUTS:"
    foreach($i in $Inputs){ Add-Str $i.name; Add-Byte $typeByte[$i.type]; Add-Size $i.size }
    Add-Str "OUTPUTS:"
    foreach($o in $Outputs){ Add-Str $o.name; Add-Byte $typeByte[$o.type]; Add-Size $o.size }
    Add-Str "REQUIRES:"
    foreach($r in $Requires){
        $parts = $r -split '@',2
        Add-Str $parts[0]; Add-Str "@"; Add-Str $parts[1]
    }
    Add-Str "GUARANTEES:"
    foreach($g in $Guarantees){ Add-Byte $guarByte[$g] }

    $data = $ms.ToArray()
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $digest = $sha.ComputeHash($data)
    $hex = ($digest[0..3] | ForEach-Object { $_.ToString('x2') }) -join ''
    return $hex
}
