class Summary {
    int raised;
    int selected;
    int delta;

    Summary(int raised, int selected, int delta) {
        this.raised = raised;
        this.selected = selected;
        this.delta = delta;
    }
}

class SummaryRequest {
    int value;
    int limit;
    int fallback;
}

class Calculator {
    static Summary summarize(SummaryRequest request) {
        int raised = request.value + 1;
        int selected = raised > request.limit ? raised : request.fallback;
        int delta = selected - raised;
        Summary result = new Summary(raised, selected, 0);
        result.delta = delta;
        return result;
    }
}
