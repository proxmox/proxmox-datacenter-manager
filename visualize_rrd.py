#!/usr/bin/env python3
# deps: python3-matplotlib python3-pandas python3-requests
#
# Usage example:
# ```
# export PDM_USERNAME="root@pam"
# export PDM_PASSWORD="<password>"
# export PDM_URL="https://172.30.0.4:8443"
#
# ./visualize_rrd.py pve/remotes/<remote>/qemu/100 cpu-current
# ./visualize_rrd.py pbs/remotes/<remote>/datastore/<store> disk-used
# ```

import os
import sys

import pandas as pd
import matplotlib.pyplot as plt
import requests

for env_var in ["PDM_URL", "PDM_USERNAME", "PDM_PASSWORD"]:
    if not os.environ.get(env_var):
        raise Exception(f"{env_var} not set")

url = os.environ["PDM_URL"]
user = os.environ["PDM_USERNAME"]
password = os.environ["PDM_PASSWORD"]

query = sys.argv[1]
rows = sys.argv[2:]

r = requests.post(
    f"{url}/api2/json/access/ticket",
    verify=False,
    data={
        "username": user,
        "password": password
    }
)

data = r.json()['data']
csrf = data['CSRFPreventionToken']
ticket = data['ticket']

r = requests.get(
    f"{url}/api2/json/{query}/rrddata",
    params={"cf": "AVERAGE", "timeframe": "hour"},
    cookies={"PDMAuthCookie": ticket},
    verify=False
)

data = r.json()
df = pd.DataFrame(data['data'])
df.plot(x='time', y=rows)
plt.show()
