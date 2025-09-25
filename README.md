# hc-nb-api-client



```yaml
apiVersion: v1
kind: Secret
metadata:
  name: hc-client-secrets
  namespace: health-check
stringData:
  APIVERSION: v4
  TOKEN: Linode PAT 
  LOCATION: us-ord
  MAINDB_PASSWORD: DB password for main database
  MAINDB_HOSTPORT: 1.2.3.4:12345
  LOCALDB_PASSWORD: Local DB that runs in local DC
  LOCALDB_HOSTPORT: 9.8.7.8:12345
```
