ARG baseImage="unityci/base"
FROM $baseImage
ARG hubVersion="3.7.0"

# Hub dependencies
RUN apt-get -q update \
 && apt-get -q install -y --no-install-recommends --allow-downgrades zenity libgbm1 gnupg \
 && apt-get clean

# Install Unity Hub
# https://docs.unity3d.com/hub/manual/InstallHub.html#install-hub-linux
RUN wget -qO - https://hub.unity3d.com/linux/keys/public | gpg --dearmor | tee /usr/share/keyrings/Unity_Technologies_ApS.gpg > /dev/null \
 && sh -c 'echo "deb [signed-by=/usr/share/keyrings/Unity_Technologies_ApS.gpg] https://hub.unity3d.com/linux/repos/deb stable main" > /etc/apt/sources.list.d/unityhub.list' \
 && sh -c 'echo "deb [signed-by=/usr/share/keyrings/Unity_Technologies_ApS.gpg] https://hub.unity3d.com/linux/repos/deb unstable main" > /etc/apt/sources.list.d/unityhub-beta.list' \
 && apt-get -q update \
 && apt-get -q install -y "unityhub=$hubVersion" \
 && apt-get clean

# Bug in xvfb-run that causes it to redirect stderr to stdout. We want it separate
RUN sed -i 's/^\(.*DISPLAY=:.*XAUTHORITY=.*\)\( "\$@" \)2>&1$/\1\2/' /usr/bin/xvfb-run

# Alias to "unity-hub" with default params
RUN echo '#!/bin/bash\nxvfb-run -a /opt/unityhub/unityhub-bin --no-sandbox --headless "$@" 2>/dev/null' > /usr/bin/unity-hub \
 && chmod +x /usr/bin/unity-hub

# Configure
RUN mkdir -p "${UNITY_PATH}/editors" \
 && unity-hub install-path --set "${UNITY_PATH}/editors/" \
 && find /tmp -mindepth 1 -delete
