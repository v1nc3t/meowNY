package commons;

import org.junit.jupiter.api.*;

import static org.junit.jupiter.api.Assertions.*;

public class BudgetTest {

    private Budget testBudget1;
    private Budget testBudget2;
    private Budget testBudget3;

    private final ExpenseCategory testCategory = new ExpenseCategory("food");
    private final User testUser = new User();

    @BeforeEach
    void setUp() {
        testBudget1 = new Budget(50.0, 11, 2000, testCategory, testUser);
        testBudget2 = new Budget(100.0, 4, 2010, testCategory, testUser);
        testBudget3 = new Budget(100.0, 4, 2010, testCategory, testUser);
    }

    @Test
    void invalidConstructorTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testBudget1 = new Budget(-1.0, 10, 2000, testCategory, testUser),
                "invalid limit"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testBudget1 = new Budget(1.0, 13, 2000, testCategory, testUser),
                "invalid month"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testBudget1 = new Budget(1.0, 10, 10000, testCategory, testUser),
                "invalid year"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testBudget1 = new Budget(1.0, 10, 2000, null, testUser),
                "null category"
        );

        assertThrows(IllegalArgumentException.class, () ->
                testBudget1 = new Budget(1.0, 10, 2000, testCategory, null),
                "null user"
        );
    }

    @Test
    void setIdTest() {
        Long id = 1L;
        testBudget1.setId(id);
        assertEquals(id, testBudget1.getId());
    }

    @Test
    void setValidUserTest() {
        testBudget1.setUser(testUser);
        assertEquals(testUser, testBudget1.getUser());
    }

    @Test
    void setInvalidUserTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testBudget1.setUser(null)
        );
    }

    @Test
    void setValidLimitAmountTest() {
        double limit = 100.0;
        testBudget1.setLimitAmount(limit);
        assertEquals(limit, testBudget1.getLimitAmount());
    }

    @Test
    void setInvalidLimitAmountTest() {
        double limit = -100.0;
        assertThrows(IllegalArgumentException.class, () ->
                testBudget1.setLimitAmount(limit)
        );
    }

    @Test
    void setValidMonthTest() {
        int month = 5;
        testBudget1.setMonth(month);
        assertEquals(month, testBudget1.getMonth());
    }

    @Test
    void setInvalidMonthTest() {
        int month = 13;
        assertThrows(IllegalArgumentException.class, () ->
                testBudget1.setMonth(month)
        );
    }

    @Test
    void setValidYearTest() {
        int year = 1950;
        testBudget1.setYear(year);
        assertEquals(year, testBudget1.getYear());
    }

    @Test
    void setInvalidYearTest() {
        int year = 1900;
        assertThrows(IllegalArgumentException.class, () ->
                testBudget1.setYear(year)
        );
    }

    @Test
    void setNullCategory() {
        ExpenseCategory newCategory = new ExpenseCategory("travel");
        testBudget1.setCategory(newCategory);
        assertEquals(newCategory, testBudget1.getCategory());
    }

    @Test
    void setNotNullCategory() {
        assertThrows(IllegalArgumentException.class, () ->
                testBudget1.setCategory(null)
        );
    }

    @Test
    void notEqualsTest() {
        assertNotEquals(testBudget1, testBudget2);
    }

    @Test
    void equalsTest() {
        assertEquals(testBudget3, testBudget2);
    }

    @Test
    void hashCodeDiffTest() {
        assertNotEquals(testBudget1.hashCode(), testBudget2.hashCode());
    }

    @Test
    void hashCodeSameTest() {
        assertEquals(testBudget2.hashCode(), testBudget3.hashCode());
    }
}
