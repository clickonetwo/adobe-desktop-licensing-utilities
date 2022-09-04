$cmd = $args[0];
$serviceName = "FRL Online Proxy";
$nssm = Join-Path (Join-Path $PSScriptRoot bin) nssm.exe
if (![System.IO.File]::Exists($nssm)) {
    echo "You must place this script next to the proxy executable and bin folder"
}

switch ($cmd) {
    start {
        $proxy = "$PSScriptRoot\frl-proxy.exe"
        if (![System.IO.File]::Exists($proxy)) {
            echo "You must place this script next to the proxy executable";
            exit 1;
        }
        $cfg = "$PSScriptRoot\proxy-conf.toml"
        if (![System.IO.File]::Exists($cfg)) {
            echo "You must have run frl-proxy configure before this script";
            exit 1;
        }
        $stdout = "$PSScriptRoot\proxy-service-stdout.log"
        $stderr = "$PSScriptRoot\proxy-service-stderr.log"
        & $nssm install $serviceName $proxy start;
        & $nssm set $serviceName AppStdout $stdout
        & $nssm set $serviceName AppStderr $stderr
        & $nssm start $serviceName;
    }
    stop {
        & $nssm stop $serviceName;
    }
    restart {
        & $nssm restart $serviceName;
    }
    remove {
        & $nssm stop $serviceName;
        & $nssm remove $serviceName confirm;
    }
    Default {
        echo "Command (start, stop, restart, remove) is required.";
        exit 1;
    }
}
exit 0;
