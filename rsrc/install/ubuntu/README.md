# Ubuntu Installation

To set up a clean, updated Ubuntu 22.04.x LTS machine to run the proxy, follow these steps.  They assume you have an SSH key that can
be used for a root login on the machine.  The steps must be followed in the order they appear here.

## Create an `adlu` user

Start by logging in to the target machine as root.

1. Create `adlu` user with the command `adduser adlu`.  Give this user a good password and remember it (for later use with `sudo`).
2. Give the `adlu` user `sudo` privileges with the command `usermod -aG sudo adlu`.
3. Set up ssh login for the `adlu` user with the command `rsync --archive --chown=adlu:adlu ~/.ssh /home/adlu/` This creates a copy of root's `.ssh` directory, owned by `adlu`, in the `adlu` home directory.  Thus, the same key will work both for root and for the `adlu` user.  (You can use the `-l` option with ssh to determine which user you want to log in as.)

## Create a certificate and key file

This step is expected to be performed on your local machine.

Because all clients use SSL to talk to the server, you will need a certificate for the name `lcs-ulecs.adobe.io` that all clients trust. Similarly, if you are doing FRL proxying, you will need this certificate to additionally have the name of your proxy host as configured in your FRL packages.  You may also want this certificate to have the actual DNS name of the server running the adlu-proxy.

Once you've made your certificate in PEM format, give it the name `adlu-proxy.cert`, and give its matching (non-encrypted, PEM format) key the name `adlu-proxy.key`.  Put them both (via `scp`) in the `/home/adlu` directory of your server, owned by the `adlu` user.  That's where nginx will look for them. (Since nginx runs as root, you can keep the permissions on the keyfile restricted to owner read only, as usual.)

_N.B._ Note that, since you are creating a certificate for an `adobe.io` host, you will need to use a private root CA that your clients trust to sign the certificate.  If you use an intermediate certificate between your root CA and your proxy machine certificate, then you will want your `adlu-proxy.cert` certificate to actually be a "full chain" combination that includes the intermediate as well as the proxy certificate.

## Transfer the configuration files

Copy the three files [adlu-proxy.conf](adlu-proxy.conf), [adlu-proxy.service](adlu-proxy.service), and [install-adlu-proxy-release.sh](install-adlu-proxy-release.sh) from this repository directory to the `/home/adlu` directory, so you can access them in later steps.

## Install the proxy

This step is performed logged in as `adlu` on the target machine.

1. Give the command `./install-adlu-proxy-release.sh tagname`, where _tagname_ is one of the released tags on GitHub (e.g., `v1.0.0-beta.1`).  This will create an `adlu-proxy` subdirectory and install the Ubuntu build from that release into that directory.  It will then prompt you to configure your proxy.
2. Configure the proxy to use plain HTTP on 127.0.0.1 (localhost) port 8080, because that's where nginx will direct the incoming traffic.  Do not have the proxy listen to external addresses.

_N.B._ If you have previously installed an earlier version of the proxy, your existing configuration file will not be disturbed, so you can just accept all the same options you had set before during the configuration process.

## Install and control the proxy service

This step is performed logged in as `adlu` on the target machine.

1. Execute the command `sudo mv adlu-proxy.service /etc/systemd/system/` to move the proxy's service configuration file to the system directory that holds local services.
2. Run the command `sudo systemctl enable adlu-proxy` to register the proxy with the system drivers.  This will ensure that it is started on boot, and terminated cleanly on orderly shutdown.
3. To start the proxy service, run the command `sudo systemctl start adlu-proxy`.
4. To check on the proxy service, run the command `systemctl status adlu-proxy` (or `sudo systemctl status adlu-proxy`, if you want to see the boot-time start journal).
5. To stop the proxy service, run the command `sudo systemctl stop adlu-proxy`.
6. To unregister the proxy service, so it does not start at boot, run the command `sudo systemctl disable adlu-proxy`.

## Configure nginx

This step is performed logged in as `adlu` on the target machine. 

1. Install nginx via `sudo apt install nginx`. This will run nginx as a service on the machine.
2. \[Note: This step is only needed if (a) you have another process on your server that is listening via nginx to inbound HTTPS traffic, and (b) you are using an FRL configuration that names the proxy server.\]  Edit the `adlu-proxy.conf` file to make sure that your FRL proxy server name is included in the `server_name` directive.  There are comments in the file that explain how to do this.
3. Execute the command `sudo mv adlu-proxy.conf /etc/nginx/conf.d/` to install the ADLU reverse proxy configuration into nginx.  (This can be done while nginx is running, and will not take effect until nginx is restarted.)
4. \[Note: This step is only needed if you performed step 2.] Execute the command `sudo nginx -t` to make sure your configuration file edit didn't break anything.
5. Restart nginx via `sudo systemctl restart nginx`
6. Check the nginx status via `systemctl status nginx` to make sure there were no problems.

## Test the installation

Once you've completed the above steps, you can test the installation by going to the proxy's status endpoint in your local machine's browser.

If you are proxying logs:

   1. Make sure you have a hosts file entry for `lcs-ulecs.adobe.io` (or `lcs-cops.adobe.io`)on your local machine that points to your server's IP address.
   2. Navigate to `https://lcs-ulecs.adobe.io/status` (or `https://lcs-cops.adobe.io/status`) in your local machine's browser.  You should see the proxy's status message.

If you are proxying FRL:

   1. Make sure you have a hosts file entry for your FRL server (either `lcs-cops.adobe.io` or your configured proxy) on your local machine that points to your server's IP address.
   2. Navigate to `https://server/status` in your local machine's browser (where _server_ is the DNS name of the server).  You should see the proxy's status message.

If you are proxying both logs and FRL through the same server, you can use either of the above instructions.  The status will be the same for both.
