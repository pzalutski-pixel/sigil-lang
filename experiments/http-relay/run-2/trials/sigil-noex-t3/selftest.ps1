. "$PSScriptRoot\allhashes.ps1" *> $null  # loads Compute-Hash + F, also writes hashes.txt
# Re-validate against known stdlib hashes
$p = Compute-Hash -Inputs @((F 'data' 'bytes' 65536),(F 'length' 'int' 8)) -Outputs @((F 'bytes_written' 'int' 8)) -Requires @() -Guarantees @('writes_output')
$n = Compute-Hash -Inputs @() -Outputs @((F 'timestamp' 'int' 8)) -Requires @() -Guarantees @('writes_output')
$ng = Compute-Hash -Inputs @((F 'value' 'int' 8)) -Outputs @((F 'result' 'int' 8)) -Requires @() -Guarantees @('pure','no_alloc','writes_output')
$ft = Compute-Hash -Inputs @((F 'timestamp' 'int' 8)) -Outputs @((F 'formatted' 'bytes' 32),(F 'length' 'int' 8)) -Requires @() -Guarantees @('writes_output')
$op = Compute-Hash -Inputs @((F 'path' 'bytes' 4096),(F 'flags' 'int' 8)) -Outputs @((F 'fd' 'int' 8)) -Requires @() -Guarantees @('writes_output')
Write-Host "print: $p expect 66227b6e"
Write-Host "now:   $n expect cee78134"
Write-Host "isneg: $ng expect b15b0b3a"
Write-Host "ftime: $ft expect 83ebda2d"
Write-Host "open:  $op expect 31f81967"
