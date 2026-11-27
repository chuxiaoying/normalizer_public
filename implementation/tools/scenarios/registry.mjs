// The same scene inventory drives interpretation and E2E entry admission.
const frontendScenarios = new Map([
  ['frontend-vo-production', 'event'],
  ['frontend-vo-consumption', 'projection'],
]);

export function frontendScenario(scene) {
  return frontendScenarios.get(scene);
}
