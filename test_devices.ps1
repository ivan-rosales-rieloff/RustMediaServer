# DLNA Device Simulation Test Script
# Tests ContentDirectory Browse with different device User-Agents

Write-Host "=== DLNA Device Simulation Tests ===" -ForegroundColor Cyan
Write-Host ""

$baseUrl = "http://localhost:3000"

# Test 1: GetProtocolInfo (no User-Agent needed)
Write-Host "[TEST 1] GetProtocolInfo" -ForegroundColor Yellow
$protoHeaders = @{
    "Content-Type" = 'text/xml; charset="utf-8"'
    "SOAPAction" = '"urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo"'
}
$protoBody = @"
<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body><u:GetProtocolInfo xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1"/></s:Body>
</s:Envelope>
"@

try {
    $result = Invoke-WebRequest -Uri "$baseUrl/services/ConnectionManager/control" -Method Post -Headers $protoHeaders -Body $protoBody -TimeoutSec 5
    Write-Host "  Status: $($result.StatusCode)" -ForegroundColor Green
    if ($result.Content -match "DLNA.ORG_PN") {
        Write-Host "  DLNA Flags: Found in response" -ForegroundColor Green
    } else {
        Write-Host "  DLNA Flags: NOT found in response" -ForegroundColor Red
    }
} catch {
    Write-Host "  Error: $_" -ForegroundColor Red
}

Write-Host ""

# ContentDirectory Browse template
$browseBody = @"
<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body>
<u:Browse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1">
<ObjectID>0</ObjectID>
<BrowseFlag>BrowseDirectChildren</BrowseFlag>
<Filter>*</Filter>
<StartingIndex>0</StartingIndex>
<RequestedCount>100</RequestedCount>
<SortCriteria></SortCriteria>
</u:Browse>
</s:Body>
</s:Envelope>
"@

# Test 2: LG WebOS
Write-Host "[TEST 2] LG WebOS TV Browse" -ForegroundColor Yellow
$lgHeaders = @{
    "Content-Type" = 'text/xml; charset="utf-8"'
    "SOAPAction" = '"urn:schemas-upnp-org:service:ContentDirectory:1#Browse"'
    "User-Agent" = "Linux/3.10.19-32.afro.4 UPnP/1.0 LGE WebOS TV LGE_DLNA_SDK/1.6.0/04.30.13 DLNADOC/1.50"
}

try {
    $result = Invoke-WebRequest -Uri "$baseUrl/services/ContentDirectory/control" -Method Post -Headers $lgHeaders -Body $browseBody -TimeoutSec 5
    Write-Host "  Status: $($result.StatusCode)" -ForegroundColor Green
    if ($result.Content -match "DLNA.ORG_PN") {
        Write-Host "  DLNA Flags: Found in response" -ForegroundColor Green
    } elseif ($result.Content -match "BrowseResponse") {
        Write-Host "  Browse Response: Valid (no media files)" -ForegroundColor Green
    }
} catch {
    Write-Host "  Error: $_" -ForegroundColor Red  
}

Write-Host ""

# Test 3: Samsung 9 Series  
Write-Host "[TEST 3] Samsung 9 Series Browse" -ForegroundColor Yellow
$samsungHeaders = @{
    "Content-Type" = 'text/xml; charset="utf-8"'
    "SOAPAction" = '"urn:schemas-upnp-org:service:ContentDirectory:1#Browse"'
    "User-Agent" = "DLNADOC/1.50 SEC_HHP_[TV] Samsung 9 Series (65)/1.0 UPnP/1.0"
}

try {
    $result = Invoke-WebRequest -Uri "$baseUrl/services/ContentDirectory/control" -Method Post -Headers $samsungHeaders -Body $browseBody -TimeoutSec 5
    Write-Host "  Status: $($result.StatusCode)" -ForegroundColor Green
    if ($result.Content -match "BrowseResponse") {
        Write-Host "  Browse Response: Valid" -ForegroundColor Green
    }
} catch {
    Write-Host "  Error: $_" -ForegroundColor Red
}

Write-Host ""

# Test 4: Roku TV
Write-Host "[TEST 4] Roku TV Browse" -ForegroundColor Yellow
$rokuHeaders = @{
    "Content-Type" = 'text/xml; charset="utf-8"'
    "SOAPAction" = '"urn:schemas-upnp-org:service:ContentDirectory:1#Browse"'
    "User-Agent" = "Roku/5000X-7"
}

try {
    $result = Invoke-WebRequest -Uri "$baseUrl/services/ContentDirectory/control" -Method Post -Headers $rokuHeaders -Body $browseBody -TimeoutSec 5
    Write-Host "  Status: $($result.StatusCode)" -ForegroundColor Green
    if ($result.Content -match "BrowseResponse") {
        Write-Host "  Browse Response: Valid" -ForegroundColor Green
    }
} catch {
    Write-Host "  Error: $_" -ForegroundColor Red
}

Write-Host ""
Write-Host "=== Tests Complete ===" -ForegroundColor Cyan
