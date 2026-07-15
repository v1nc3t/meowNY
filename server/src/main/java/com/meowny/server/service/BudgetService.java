package com.meowny.server.service;

import com.meowny.commons.entity.Budget;
import com.meowny.commons.entity.Category;
import com.meowny.commons.entity.User;
import com.meowny.server.dto.budget.BudgetResponse;
import com.meowny.server.dto.budget.CreateBudgetRequest;
import com.meowny.server.dto.budget.UpdateBudgetRequest;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.BudgetRepository;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.UserRepository;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.stream.Collectors;

public class BudgetService {

    private final BudgetRepository budgetRepository;
    private final UserRepository userRepository;
    private final CategoryRepository categoryRepository;

    public BudgetService(BudgetRepository budgetRepository,
                         UserRepository userRepository,
                         CategoryRepository categoryRepository
    ) {
        this.budgetRepository = budgetRepository;
        this.userRepository = userRepository;
        this.categoryRepository = categoryRepository;
    }

    @Transactional(readOnly = true)
    public BudgetResponse getBudgetById(Long id) {
        Budget budget = budgetRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Budget not found with ID:" + id));
        return mapToResponse(budget);
    }

    @Transactional(readOnly = true)
    public List<BudgetResponse> getBudgetsByPeriod(Long userId, Integer year, Integer month) {
        return budgetRepository.findByUserIdAndYearAndMonth(userId, year, month)
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional
    public BudgetResponse createBudget(CreateBudgetRequest request) {
        User user = userRepository.findById(request.userId())
                .orElseThrow(() -> new IllegalArgumentException("User not found with ID: " + request.userId()));

        Category category = categoryRepository.findById(request.categoryId())
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + request.categoryId()));

        if (!category.getUser().getId().equals(request.userId())) {
            throw new IllegalArgumentException("Category must belong to the specified user.");
        }

        budgetRepository.findByUserIdAndCategoryIdAndMonthAndYear(
                        request.userId(), request.categoryId(), request.month(), request.year())
                .ifPresent(existing -> {
                    throw new ResourceConflictException(
                            "A budget limit is already defined for category " + category.getName() +
                                    " in " + request.month() + "/" + request.year() + "."
                    );
                });

        Budget budget = new Budget();
        budget.setUser(user);
        budget.setCategory(category);
        budget.setLimitAmount(request.limitAmount());
        budget.setMonth(request.month());
        budget.setYear(request.year());

        Budget savedBudget = budgetRepository.save(budget);
        return mapToResponse(savedBudget);
    }

    @Transactional
    public BudgetResponse updateBudget(Long id, UpdateBudgetRequest request) {
        Budget budget = budgetRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Budget not found with ID:" + id));

        budget.setLimitAmount(request.limitAmount());
        Budget updatedBudget = budgetRepository.save(budget);
        return mapToResponse(updatedBudget);
    }

    @Transactional
    public void deleteBudget(Long id) {
        if (!budgetRepository.existsById(id)) {
            throw new IllegalArgumentException("Budget not found with ID: " + id);
        }
        budgetRepository.deleteById(id);
    }

    private BudgetResponse mapToResponse(Budget budget) {
        return new BudgetResponse(
                budget.getId(),
                budget.getUser().getId(),
                budget.getCategory().getId(),
                budget.getCategory().getName(),
                budget.getLimitAmount(),
                budget.getMonth(),
                budget.getYear(),
                budget.getCreatedAt(),
                budget.getUpdatedAt()
        );
    }

}
