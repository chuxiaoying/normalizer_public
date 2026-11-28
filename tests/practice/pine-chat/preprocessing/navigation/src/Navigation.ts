// One selected App.vue instance, limited to tab and search-placeholder bindings.
/** @N entryState */
const query = { value: '' }
/** @N entryState */
const page: { value: 'chats' | 'contacts' } = { value: 'chats' }
/** @N entryState */
const problem = { value: '' }
/** @N entryState */
const notice = { value: '' }

// App.vue: the two tab callbacks bind a fixed argument, not an event parameter.
function switchPage(next: 'chats' | 'contacts') {
  page.value = next; query.value = ''; notice.value = ''; problem.value = ''
}

export function onChatsClick(): void {
  switchPage('chats')
}

export function onContactsClick(): void {
  switchPage('contacts')
}

/** @N componentProp Input#global-search.placeholder */
export function searchPlaceholder(): string {
  return page.value === 'chats' ? '搜索会话和消息' : '搜索联系人'
}
