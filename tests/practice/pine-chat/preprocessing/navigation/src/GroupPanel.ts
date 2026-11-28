// One selected App.vue instance. The group button is only bound in a group conversation.
/** @N entryState */
const panel = { value: false }

// App.vue: this button exists only in the selected group conversation.
export function onGroupSettingsClick(): void {
  panel.value = !panel.value
}

/** @N componentProp Button#group-settings.aria-expanded */
export function groupExpanded(): boolean {
  return panel.value
}
