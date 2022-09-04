# Ubuntu Installation

To set up a clean, updated Ubuntu 22.04.x LTS machine to run the proxy, follow these steps.  They assume you have an SSH key that can
be used for a root login on the machine.

## Create an `adlu` user

Logged in as root:

1. Create `adlu` user with the command `adduser adlu`.  Give this user a good password and remember it (for later use with `sudo`).
2. Give the `adlu` user `sudo` privileges with `usermod -aG sudo adlu`.
3. Set up ssh login for the `adlu` user with the command `rsync --archive --chown=adlu:adlu ~/.ssh /user/adlu/` This creates a copy of root's `.ssh` directory, owned by `adlu`, in the `adlu` home directory.  Thus, the same key will work both for root and for the `adlu` user.  (You can use the `-l` option with ssh to determine which user you want to log in as.)

From now on, you will only log in as the `adlu` user.  For convenience, start by transfering the [ubuntu directory](.) containing these instructions to the `/home/adlu` directory, so the files are available on the target machine.

## Create a certificate and key file

Because all clients use SSL to talk to the server, you will need a certificate for the name `lcs-ulecs.adobe.io` that all clients trust. Similarly, if you are doing FRL proxying, you will need this certificate to additionally have the name of your proxy host as configured in your FRL packages.  You may also want this certificate to have the actual DNS name of the server running the adlu-proxy.

Once you've made your certificate in PEM format, give it the name `adlu-proxy.cert`, and give its matching (non-encrypted, PEM format) key the name `adlu-proxy.key`.  Put them both (via `scp`) in the `/home/adlu` directory of your server, owned by the `adlu` user.  That's where nginx will look for them. (Since nginx runs as root, you can keep the permissions on the keyfile restricted to owner read only, as usual.)

_N.B._ Note that, since you are creating a certificate for an `adobe.io` host, you will need to use a private root CA that your clients trust to sign the certificate.  If you use an intermediate certificate between your root CA and your proxy machine certificate, then you will want your `adlu-proxy.cert` certificate to actually be a "full chain" combination that includes the intermediate as well as the proxy certificate.

## Install the proxy

1. Put the [install-adlu-proxy-tag.sh](install-adlu-proxy-tag.sh) file in the `/home/adlu` directory.
2. Run the script with `./install-adlu-proxy-tag.sh _tagname_`, where _tagname_ is one of the released tags on GitHub (e.g., `v1.0.0-beta.1`).  This will create an `adlu-proxy` subdirectory and install the Ubuntu build from that release into that directory.  It will then prompt you to configure your proxy.
3. Configure the proxy to use plain HTTP on 127.0.0.1 (localhost) port 8080, because that's where nginx will direct the incoming traffic.  Do not have the proxy listen to external addresses.

## Install and control the proxy service

1. Put the [adlu-proxy.service](adlu-proxy.service) file in the `/etc/systemd/system` directory.
2. From any directory, run the command `sudo systemctl enable adlu-proxy` to register the proxy with the system drivers.  This will ensure that it is started on boot, and terminated cleanly on orderly shutdown.
3. To start the proxy service, run the command `sudo systemctl start adlu-proxy`.
4. To check on the proxy service, run the command `systemctl status adlu-proxy` (or `sudo systemctl status adlu-proxy`, if you want to see the boot-time start journal).
5. To stop the proxy service, run the command `sudo systemctl stop adlu-proxy`.
6. To unregister the proxy service so it does not start at boot, run the command `sudo systemctl disable adlu-proxy`.

## Install and configure nginx

These instructions assume you are only using `nginx` to terminate SSL for your proxy.

1. Install nginx via `sudo apt install nginx`. This will run nginx as a service on the machine. 
2. Install the [nginx.conf](nginx.conf) file from this directory as `/etc/nginx/nginx.conf` (the primary nginx configuration file).
3. Restart nginx via `sudo systemctl restart nginx`
4. Check the nginx status via `systemctl status nginx` to make sure there were no problems.

## Test the installation

Once you've completed the above steps, you can test the installation by making sure you have a hosts file entry for `lcs-ulecs.adobe.io` on your local machine that points to your server's IP address, and then navigating to `https://lcs-ulecs.adobe.io/status` in your local machine's browser.  You should see the proxy's status message.
