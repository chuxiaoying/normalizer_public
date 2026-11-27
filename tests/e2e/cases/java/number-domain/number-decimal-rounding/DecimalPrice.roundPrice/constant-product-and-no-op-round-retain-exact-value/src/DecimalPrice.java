import java.math.BigDecimal;
import normalizer.intrinsics.N;

class DecimalPrice {
    static BigDecimal roundPrice(BigDecimal value) {
        BigDecimal left; BigDecimal right; /* @normalizer-ir left = 1.25 */ /* @normalizer-ir right = 2.5 */ return N.round(N.multiply(left, right), 4, N.HALF_EVEN);
    }
}
