class CaseReuse {
    static int cap(int value, int limit, int base) {
        int adjusted = value + 2;
        return adjusted > limit ? limit : adjusted + base;
    }
}
