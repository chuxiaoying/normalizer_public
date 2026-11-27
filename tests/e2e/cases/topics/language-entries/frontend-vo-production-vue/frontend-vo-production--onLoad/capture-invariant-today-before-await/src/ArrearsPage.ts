type BillRow = {
  accountId: number
  dueDate: number
  amount: number
  waivedAmount: number
  priority: number
}

type Profile = {
  accountId: number
  minPriority: number
  quota: number
}

class ProfileFetchError extends Error {}
class BillQueryError extends Error {}

/** @N entryState */
const arrears = {
  loadedAccountId: 0,
  status: {
    loading: false,
    ready: false,
    failed: false,
  },
  summary: {
    payable: 0,
  },
}

interface ProfileClient {
  /** @N rejects ProfileFetchError */
  fetchProfile(accountId: number): Promise<Profile>
}

interface ArrearsClient {
  /** @N rejects BillQueryError */
  queryOpenBills(accountId: number): Promise<BillRow[]>
}

/** @N external */
declare const profileClient: ProfileClient
/** @N external */
declare const arrearsClient: ArrearsClient

export async function onLoad(accountId: number, today: number): Promise<void> {
  arrears.loadedAccountId = accountId
  arrears.status.loading = true

  try {
    const profile = await profileClient.fetchProfile(accountId)
    const rows = await arrearsClient.queryOpenBills(profile.accountId)

    let total: number
    /* @normalizer-ir
       total = sum from rows as row
         where row.dueDate <= today
           and row.priority >= profile.minPriority
         let netAmount = row.amount - row.waivedAmount
         where netAmount > 0
         yield netAmount
    */

    arrears.status.loading = false
    arrears.summary.payable = total > profile.quota ? profile.quota : total
    arrears.status.ready = true
  } catch (error) {
    arrears.status.loading = false
    arrears.status.failed = true
    arrears.status.ready = false
    throw error
  }
}
