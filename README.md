# hc-nb-api-client

This is a client of [hc-nb-api](https://github.com/nathanle/hc-nb-api).

The intent is to run it in each datacenter where there is presence. The Secret manifest is similar to hc-nb-api, except that it has a location (LOC) defined and settings for the local database as well as the main database created with hc-nb-api.

1. Create local database

2. Create local LKE cluster

3. Create PAT for APIs

4. Configure `hc-client-secrets.yaml` 

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

5. Configure `hc-client-deployment.yaml`

```yaml

apiVersion: apps/v1
kind: Deployment
metadata:
  name: hc-nb-client 
  name: health-check
spec:
  replicas: 1
  selector:
    matchLabels:
      app: hc-nb-client 
  template:
    metadata:
      labels:
        app: hc-nb-client 
    spec:
      serviceAccountName: node-health-check-operator-rs-account
      imagePullSecrets:
      - name: ghcr-login-secret
      containers:
      - name: hc-nb-client 
          image: nathanles/hc-nb-api-client:latest
        envFrom:
        - secretRef:
            name: hc-client-secrets
        resources:
          requests:
            memory: "10Mi"
            cpu: "10m"
          limits:
            memory: "20Mi"
            cpu: "100m"
        imagePullPolicy: IfNotPresent
```


6. Apply deployment
