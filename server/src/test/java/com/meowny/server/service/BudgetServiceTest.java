package com.meowny.server.service;

import com.meowny.server.dto.budget.BudgetResponse;
import com.meowny.server.dto.budget.CreateBudgetRequest;
import com.meowny.server.dto.budget.UpdateBudgetRequest;
import com.meowny.server.entity.Budget;
import com.meowny.server.entity.BudgetScope;
import com.meowny.server.entity.Category;
import com.meowny.server.entity.TransactionType;
import com.meowny.server.entity.User;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.exception.ResourceNotFoundException;
import com.meowny.server.repository.BudgetRepository;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.security.CurrentUserService;
import com.meowny.server.support.TestCurrentUserSupport;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.util.List;
import java.util.Optional;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.*;

@ExtendWith(MockitoExtension.class)
class BudgetServiceTest {

    @Mock
    private BudgetRepository budgetRepository;

    @Mock
    private CategoryRepository categoryRepository;

    @Mock
    private CurrentUserService currentUserService;

    @InjectMocks
    private BudgetService budgetService;

    private User currentUser;

    @BeforeEach
    void setUp() {
        currentUser = new User();
        currentUser.setId(1L);
        TestCurrentUserSupport.stubCurrentUser(currentUserService, currentUser);
    }

    @Test
    @DisplayName("getBudgetById: Should return BudgetResponse when budget exists")
    void getBudgetById_BudgetExists_ReturnsResponse() {
        Long budgetId = 100L;
        Budget budget = createMockCategoryBudget(budgetId, 1L, 2L, "Groceries", BigDecimal.valueOf(500), LocalDate.of(2026, 5, 1));

        when(budgetRepository.findById(budgetId)).thenReturn(Optional.of(budget));

        BudgetResponse response = budgetService.getBudgetById(budgetId);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(budgetId);
        assertThat(response.categoryName()).isEqualTo("Groceries");
        verify(budgetRepository).findById(budgetId);
    }

    @Test
    @DisplayName("getBudgetById: Should throw ResourceNotFoundException when budget not found")
    void getBudgetById_BudgetNotFound_ThrowsException() {
        Long budgetId = 100L;
        when(budgetRepository.findById(budgetId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.getBudgetById(budgetId))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("getCurrentUserBudgetsByPeriod: Should return list of budget responses")
    void getCurrentUserBudgetsByPeriod_ValidInput_ReturnsList() {
        Integer year = 2026;
        Integer month = 5;
        LocalDate effectiveFrom = LocalDate.of(year, month, 1);
        Budget budget = createMockCategoryBudget(100L, 1L, 2L, "Rent", BigDecimal.valueOf(1200), effectiveFrom);

        when(budgetRepository.findByUserIdAndEffectiveFrom(1L, effectiveFrom))
                .thenReturn(List.of(budget));

        List<BudgetResponse> results = budgetService.getCurrentUserBudgetsByPeriod(year, month);

        assertThat(results).hasSize(1);
        assertThat(results.get(0).limitAmount()).isEqualTo(BigDecimal.valueOf(1200));
        verify(budgetRepository).findByUserIdAndEffectiveFrom(1L, effectiveFrom);
    }

    @Test
    @DisplayName("createBudget: Should create category budget successfully under valid conditions")
    void createBudget_ValidRequest_CreatesBudget() {
        LocalDate effectiveFrom = LocalDate.of(2026, 5, 15);
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.CATEGORY, 2L, BigDecimal.valueOf(300), effectiveFrom);

        User user = new User();
        user.setId(1L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(user);
        category.setType(TransactionType.EXPENSE);
        category.setName("Utilities");

        Budget savedBudget = createMockCategoryBudget(100L, 1L, 2L, "Utilities", BigDecimal.valueOf(300), LocalDate.of(2026, 5, 1));

        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(budgetRepository.findByUserIdAndCategoryIdAndEffectiveFrom(1L, 2L, LocalDate.of(2026, 5, 1)))
                .thenReturn(Optional.empty());
        when(budgetRepository.save(any(Budget.class))).thenReturn(savedBudget);

        BudgetResponse response = budgetService.createBudget(request);

        assertThat(response).isNotNull();
        assertThat(response.id()).isEqualTo(100L);
        verify(budgetRepository).save(any(Budget.class));
    }

    @Test
    @DisplayName("createBudget: Should create a global budget when no category is provided")
    void createBudget_GlobalScope_CreatesBudget() {
        LocalDate effectiveFrom = LocalDate.of(2026, 5, 1);
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.GLOBAL, null, BigDecimal.valueOf(2000), effectiveFrom);

        Budget savedBudget = createMockGlobalBudget(100L, 1L, BigDecimal.valueOf(2000), effectiveFrom);

        when(budgetRepository.findGlobalByUserIdAndEffectiveFrom(1L, effectiveFrom)).thenReturn(Optional.empty());
        when(budgetRepository.save(any(Budget.class))).thenReturn(savedBudget);

        BudgetResponse response = budgetService.createBudget(request);

        assertThat(response).isNotNull();
        assertThat(response.scope()).isEqualTo(BudgetScope.GLOBAL);
        assertThat(response.categoryId()).isNull();
        verifyNoInteractions(categoryRepository);
    }

    @Test
    @DisplayName("createBudget: Should throw ResourceNotFoundException when category does not exist")
    void createBudget_CategoryNotFound_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.CATEGORY, 99L, BigDecimal.valueOf(300), LocalDate.of(2026, 5, 1));
        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("createBudget: Should throw ResourceNotFoundException when category belongs to a different user")
    void createBudget_CategoryBelongsToAnotherUser_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.CATEGORY, 2L, BigDecimal.valueOf(300), LocalDate.of(2026, 5, 1));

        User wrongUser = new User();
        wrongUser.setId(55L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(wrongUser);

        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(ResourceNotFoundException.class);
    }

    @Test
    @DisplayName("createBudget: Should throw IllegalArgumentException when category type is INCOME")
    void createBudget_CategoryNotExpense_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.CATEGORY, 2L, BigDecimal.valueOf(300), LocalDate.of(2026, 5, 1));

        User user = new User();
        user.setId(1L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(user);
        category.setType(TransactionType.INCOME);

        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Budgets can only be created for EXPENSE categories.");
    }

    @Test
    @DisplayName("createBudget: Should throw IllegalArgumentException when CATEGORY scope is missing a category ID")
    void createBudget_CategoryScopeMissingCategoryId_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.CATEGORY, null, BigDecimal.valueOf(300), LocalDate.of(2026, 5, 1));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Category ID is required for CATEGORY scope budgets.");
    }

    @Test
    @DisplayName("createBudget: Should throw IllegalArgumentException when GLOBAL scope includes a category ID")
    void createBudget_GlobalScopeWithCategory_ThrowsException() {
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.GLOBAL, 2L, BigDecimal.valueOf(300), LocalDate.of(2026, 5, 1));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(IllegalArgumentException.class)
                .hasMessageContaining("Global budgets cannot be assigned to a category.");
    }

    @Test
    @DisplayName("createBudget: Should throw ResourceConflictException when a category budget already exists for the effective date")
    void createBudget_BudgetAlreadyExists_ThrowsResourceConflictException() {
        LocalDate effectiveFrom = LocalDate.of(2026, 5, 1);
        CreateBudgetRequest request = new CreateBudgetRequest(BudgetScope.CATEGORY, 2L, BigDecimal.valueOf(300), effectiveFrom);

        User user = new User();
        user.setId(1L);

        Category category = new Category();
        category.setId(2L);
        category.setUser(user);
        category.setType(TransactionType.EXPENSE);
        category.setName("Dining");

        when(categoryRepository.findById(request.categoryId())).thenReturn(Optional.of(category));
        when(budgetRepository.findByUserIdAndCategoryIdAndEffectiveFrom(1L, 2L, effectiveFrom))
                .thenReturn(Optional.of(new Budget()));

        assertThatThrownBy(() -> budgetService.createBudget(request))
                .isInstanceOf(ResourceConflictException.class)
                .hasMessageContaining("A budget limit is already defined for category Dining on 2026-05-01.");

        verify(budgetRepository, never()).save(any());
    }

    @Test
    @DisplayName("updateBudget: Should update limit amount successfully when budget exists")
    void updateBudget_BudgetExists_UpdatesSuccessfully() {
        Long budgetId = 100L;
        UpdateBudgetRequest request = new UpdateBudgetRequest(BigDecimal.valueOf(750));
        Budget existingBudget = createMockCategoryBudget(budgetId, 1L, 2L, "Leisure", BigDecimal.valueOf(500), LocalDate.of(2026, 5, 1));

        when(budgetRepository.findById(budgetId)).thenReturn(Optional.of(existingBudget));
        when(budgetRepository.save(existingBudget)).thenReturn(existingBudget);

        BudgetResponse response = budgetService.updateBudget(budgetId, request);

        assertThat(response).isNotNull();
        verify(budgetRepository).save(existingBudget);
        assertThat(existingBudget.getLimitAmount()).isEqualTo(BigDecimal.valueOf(750));
    }

    @Test
    @DisplayName("updateBudget: Should throw ResourceNotFoundException when budget to update is not found")
    void updateBudget_BudgetNotFound_ThrowsException() {
        Long budgetId = 100L;
        UpdateBudgetRequest request = new UpdateBudgetRequest(BigDecimal.valueOf(750));
        when(budgetRepository.findById(budgetId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.updateBudget(budgetId, request))
                .isInstanceOf(ResourceNotFoundException.class);

        verify(budgetRepository, never()).save(any());
    }

    @Test
    @DisplayName("deleteBudget: Should delete target budget when it exists")
    void deleteBudget_BudgetExists_DeletesSuccessfully() {
        Long budgetId = 100L;
        Budget budget = createMockCategoryBudget(budgetId, 1L, 2L, "Leisure", BigDecimal.valueOf(500), LocalDate.of(2026, 5, 1));
        when(budgetRepository.findById(budgetId)).thenReturn(Optional.of(budget));

        budgetService.deleteBudget(budgetId);

        verify(budgetRepository).delete(budget);
    }

    @Test
    @DisplayName("deleteBudget: Should throw ResourceNotFoundException when budget record doesn't exist")
    void deleteBudget_BudgetDoesNotExist_ThrowsException() {
        Long budgetId = 100L;
        when(budgetRepository.findById(budgetId)).thenReturn(Optional.empty());

        assertThatThrownBy(() -> budgetService.deleteBudget(budgetId))
                .isInstanceOf(ResourceNotFoundException.class);

        verify(budgetRepository, never()).delete(any());
    }

    private Budget createMockCategoryBudget(Long id, Long userId, Long categoryId, String categoryName,
                                            BigDecimal limit, LocalDate effectiveFrom) {
        User user = new User();
        user.setId(userId);

        Category category = new Category();
        category.setId(categoryId);
        category.setName(categoryName);

        Budget budget = new Budget();
        budget.setId(id);
        budget.setUser(user);
        budget.setScope(BudgetScope.CATEGORY);
        budget.setCategory(category);
        budget.setLimitAmount(limit);
        budget.setEffectiveFrom(effectiveFrom);
        return budget;
    }

    private Budget createMockGlobalBudget(Long id, Long userId, BigDecimal limit, LocalDate effectiveFrom) {
        User user = new User();
        user.setId(userId);

        Budget budget = new Budget();
        budget.setId(id);
        budget.setUser(user);
        budget.setScope(BudgetScope.GLOBAL);
        budget.setLimitAmount(limit);
        budget.setEffectiveFrom(effectiveFrom);
        return budget;
    }
}
