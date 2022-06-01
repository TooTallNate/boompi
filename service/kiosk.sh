#!/bin/bash

# Disable screensaver / blanking
xset s noblank
xset -dpms
xset s off

# Hide mouse
unclutter -idle 0.01 -root &

# Disable chrome error messages
sed -i 's/"exited_cleanly":false/"exited_cleanly":true/' /home/pi/.config/chromium/Default/Preferences
sed -i 's/"exit_type":"Crashed"/"exit_type":"Normal"/' /home/pi/.config/chromium/Default/Preferences

# Play silence forever to keep sound card "active" and prevent crackling sound
ffplay -f s16le -acodec pcm_s16le -hide_banner -loglevel panic -nodisp /dev/zero &

# Start Boompi WebSocket backend server
pushd /home/pi/boompi/backend
DEBUG="boompi:*" NODE_ENV="production" /usr/local/bin/node dist/main.js > log.txt 2>&1 </dev/null &
popd

# Start chromium
/usr/bin/chromium \
	--noerrdialogs \
	--disable-infobars \
	--kiosk \
	--incognito \
	--disable-pinch \
	--overscroll-history-navigation=0 \
	http://localhost:3000
