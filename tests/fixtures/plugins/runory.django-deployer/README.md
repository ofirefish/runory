# Runory Django Deployer (fixture)

Official plugin for deploying Django apps with Git + venv + Gunicorn + systemd + Nginx.

Layout on target host:

```text
/opt/runory/apps/{projectName}/
  releases/{releaseId}/
  shared/
  current -> releases/{releaseId}
```

Build package:

```text
python scripts/pack-django-plugin.py
```
