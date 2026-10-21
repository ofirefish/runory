export function deploymentConflictErrorKey(code: string): string | undefined {
  switch (code) {
    case "DEPLOYMENT_NAME_CONFLICT": return "deployment.validation.nameConflict";
    case "DEPLOYMENT_PATH_CONFLICT": return "deployment.validation.pathConflict";
    case "DEPLOYMENT_SERVICE_CONFLICT": return "deployment.validation.serviceConflict";
    case "DEPLOYMENT_GIT_TARGET_NOT_EMPTY": return "deployment.python.gitTargetNotEmpty";
    case "DEPLOYMENT_GIT_PATH_DENIED": return "deployment.python.gitPathDenied";
    default: return undefined;
  }
}
