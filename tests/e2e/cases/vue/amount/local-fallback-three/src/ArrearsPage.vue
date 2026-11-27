<template>
  <PayableCard
    id="balance"
    :amount="arrears.status.ready ? arrears.summary.payable : 3"
    :caption="arrears.status.loading ? '加载中' : '应付金额'"
  />
</template>

<script setup>
import { reactive } from 'vue'
import PayableCard from './PayableCard.vue'

const arrears = reactive({
  status: { ready: false, loading: false },
  summary: { payable: 0 }
})
</script>
