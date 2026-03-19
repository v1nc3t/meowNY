package commons;

import org.junit.jupiter.api.*;

import java.time.LocalDate;

import static org.junit.jupiter.api.Assertions.*;

public class ExpenseTest {

    private Expense testExpense1;
    private Expense testExpense2;
    private Expense testExpense3;

    private final LocalDate testDate = LocalDate.of(10000, 1, 1);
    private final User testUser = new User();
    private final ExpenseCategory testCategory = new ExpenseCategory("food");
    private final Frequency testFrequency = Frequency.WEEKLY;

    @BeforeEach
    void setUp() {
        testExpense1 = new Expense(
                "bread", 1.0, testDate, "stale", testCategory, testUser
        );

        testExpense2 = new Expense(
                "burger", 5.0, testDate, "yum", testCategory, testUser
        );

        testExpense3 = new Expense(
                "burger", 5.0, testDate, "yum", testCategory, testUser
        );
    }

    @Test
    void invalidConstructorTest() {
        assertThrows(IllegalArgumentException.class, () -> testExpense1 = new Expense(
                null, 1.0, testDate, "stale", testCategory, testUser
        ), "name null");

        assertThrows(IllegalArgumentException.class, () -> testExpense1 = new Expense(
                "bread", null, testDate, "stale", testCategory, testUser
        ), "amount null");

        assertThrows(IllegalArgumentException.class, () -> testExpense1 = new Expense(
                "bread", -1.0, testDate, "stale", testCategory, testUser
        ), "amount negative");

        assertThrows(IllegalArgumentException.class, () -> testExpense1 = new Expense(
                "bread", 1.0, null, "stale", testCategory, testUser
        ), "pay date null");

        assertThrows(IllegalArgumentException.class, () -> testExpense1 = new Expense(
                "bread", 1.0, testDate, "stale", null, testUser
        ), "category null");

        assertThrows(IllegalArgumentException.class, () -> testExpense1 = new Expense(
                "bread", 1.0, testDate, "stale", testCategory, null
        ), "user null");
    }

    @Test
    void setIdTest() {
        Long id = 1L;
        testExpense1.setId(id);
        assertEquals(id, testExpense1.getId());
    }

    @Test
    void setValidUserTest() {
        User user = new User();
        testExpense1.setUser(user);
        assertEquals(user, testExpense1.getUser());
    }

    @Test
    void setInvalidUserTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpense1.setUser(null)
        );
    }

    @Test
    void setValidNameTest() {
        String name = "train";
        testExpense1.setName(name);
        assertEquals(name, testExpense1.getName());
    }

    @Test
    void setInvalidNameTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpense1.setName(null)
        );
    }

    @Test
    void setValidAmountTest() {
        Double amount = 1.0;
        testExpense1.setAmount(amount);
        assertEquals(amount, testExpense1.getAmount());
    }

    @Test
    void setNullAmountTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpense1.setAmount(null)
        );
    }

    @Test
    void setNegativeAmountTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpense1.setAmount(-10.0)
        );
    }

    @Test
    void setValidPaymentTest() {
        LocalDate date = LocalDate.of(2000, 1, 1);
        testExpense1.setPaymentDate(date);
        assertEquals(date, testExpense1.getPaymentDate());
    }

    @Test
    void setInvalidPaymentTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpense1.setPaymentDate(null)
        );
    }

    @Test
    void setDescriptionTest() {
        String description = "Some stuff";
        testExpense1.setDescription(description);
        assertEquals(description, testExpense1.getDescription());
    }

    @Test
    void setNullDescriptionTest() {
        testExpense1.setDescription(null);
        assertNull(testExpense1.getDescription());
    }

    @Test
    void setValidCategoryTest() {
        ExpenseCategory category = new ExpenseCategory("hats");
        testExpense1.setCategory(category);
        assertEquals(category, testExpense1.getCategory());
    }

    @Test
    void setInvalidCategoryTest() {
        assertThrows(IllegalArgumentException.class, () ->
                testExpense1.setCategory(null)
        );
    }

    @Test
    void setReferenceDueDateTest() {
        LocalDate date = LocalDate.of(2000, 1, 1);
        testExpense1.setReferenceDueDate(date);
        assertEquals(date, testExpense1.getReferenceDueDate());
    }

    @Test
    void setNullReferenceDueDateTest() {
        testExpense1.setReferenceDueDate(null);
        assertNull(testExpense1.getReferenceDueDate());
    }

    @Test
    void setSourceTemplateTest() {
        RecurringExpense sourceTemplate = new RecurringExpense(
                "rent", 10.0, testDate, testFrequency, testUser, testCategory
        );
        testExpense1.setSourceTemplate(sourceTemplate);
        assertEquals(sourceTemplate, testExpense1.getSourceTemplate());
    }

    @Test
    void setNullSourceTemplateTest() {
        testExpense1.setSourceTemplate(null);
        assertNull(testExpense1.getSourceTemplate());
    }

    @Test
    void notEqualsTest() {
        assertNotEquals(testExpense1, testExpense2);
    }

    @Test
    void equalsTest() {
        assertEquals(testExpense3, testExpense2);
    }

    @Test
    void hashCodeDiffTest() {
        assertNotEquals(testExpense1.hashCode(), testExpense2.hashCode());
    }

    @Test
    void hashCodeSameTest() {
        assertEquals(testExpense3.hashCode(), testExpense2.hashCode());
    }

}
