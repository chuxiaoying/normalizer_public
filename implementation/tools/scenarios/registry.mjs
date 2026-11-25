// The same scene inventory drives interpretation and E2E entry admission.
const vueScenarios = new Map([
  ['frontend-vo-production', 'event'],
  ['frontend-vo-consumption', 'projection'],
]);

export function vueScenario(scene) {
  return vueScenarios.get(scene);
}
