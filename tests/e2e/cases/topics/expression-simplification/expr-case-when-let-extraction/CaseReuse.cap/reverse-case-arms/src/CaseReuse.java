class CaseReuse {
    static int cap(int value, int limit, int base) {
        int adjusted = value + 1;
        return adjusted <= limit ? adjusted + base : limit;
    }
}
