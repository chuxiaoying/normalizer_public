class CanonicalQuota {
    static int smaller(int quota, int total) {
        if (quota <= total) return quota;
        return total;
    }
}
