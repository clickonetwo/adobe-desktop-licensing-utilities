$cmd = $args[0];
$serviceName = "FRL Proxy";

switch ($cmd) {
    start {
        $cfg = "$PSScriptRoot\config.toml"
        if (!$cfg) {
            echo "Config file required";
            exit 1;
        }
        ./bin/nssm install $serviceName "$PSScriptRoot\frl-proxy.exe" "start --config-file $cfg";
        ./bin/nssm start $serviceName;
    }
    stop {
        ./bin/nssm stop $serviceName;
    }
    restart {
        ./bin/nssm restart $serviceName;
    }
    remove {
        ./bin/nssm stop $serviceName;
        ./bin/nssm remove $serviceName confirm;
    }
    Default {
        echo "Command (start, stop, restart, remove) is required.";
        exit 1;
    }
}
exit 0;
