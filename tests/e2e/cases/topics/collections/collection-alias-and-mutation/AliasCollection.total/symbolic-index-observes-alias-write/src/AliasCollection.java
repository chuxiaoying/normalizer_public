class AliasCollection {
    static int total(int[] values, int index, int next) {
        int[] alias = values;
        alias[1] = next;
        return values[index];
    }
}
