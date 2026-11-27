class CaseReuse {
    static int difference(int[] aaa, int[] zzz, boolean enabled, int extra) {
        int offset = enabled ? extra : 0;
        int second;
        /* @normalizer-ir
           second = sum from zzz as value yield value + offset
        */
        int first;
        /* @normalizer-ir
           first = sum from aaa as value yield value + offset
        */
        return second * second - first * first;
    }
}
