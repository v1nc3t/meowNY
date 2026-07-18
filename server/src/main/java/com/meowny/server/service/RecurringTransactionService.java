package com.meowny.server.service;

import com.meowny.server.entity.Category;
import com.meowny.server.entity.RecurringTransaction;
import com.meowny.server.entity.User;
import com.meowny.server.dto.recurringtransaction.CreateRecurringTransactionRequest;
import com.meowny.server.dto.recurringtransaction.RecurringTransactionResponse;
import com.meowny.server.dto.recurringtransaction.UpdateRecurringTransactionRequest;
import com.meowny.server.exception.ResourceConflictException;
import com.meowny.server.repository.CategoryRepository;
import com.meowny.server.repository.RecurringTransactionRepository;
import com.meowny.server.repository.TransactionRepository;
import com.meowny.server.repository.UserRepository;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.stream.Collectors;

@Service
public class RecurringTransactionService {

    private final RecurringTransactionRepository recurringTransactionRepository;
    private final UserRepository userRepository;
    private final CategoryRepository categoryRepository;
    private final TransactionRepository transactionRepository;

    public RecurringTransactionService(RecurringTransactionRepository recurringTransactionRepository,
                                       UserRepository userRepository,
                                       CategoryRepository categoryRepository,
                                       TransactionRepository transactionRepository) {
        this.recurringTransactionRepository = recurringTransactionRepository;
        this.userRepository = userRepository;
        this.categoryRepository = categoryRepository;
        this.transactionRepository = transactionRepository;
    }

    @Transactional(readOnly = true)
    public List<RecurringTransactionResponse> getTemplatesByUserId(Long userId) {
        return recurringTransactionRepository.findByUserId(userId)
                .stream()
                .map(this::mapToResponse)
                .collect(Collectors.toList());
    }

    @Transactional(readOnly = true)
    public RecurringTransactionResponse getTemplateById(Long id) {
        RecurringTransaction template = recurringTransactionRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Recurring template not found with ID: " + id));
        return mapToResponse(template);
    }

    @Transactional
    public RecurringTransactionResponse createTemplate(CreateRecurringTransactionRequest request) {
        User user = userRepository.findById(request.userId())
                .orElseThrow(() -> new IllegalArgumentException("User not found with ID: " + request.userId()));

        Category category = categoryRepository.findById(request.categoryId())
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + request.categoryId()));

        if (!category.getUser().getId().equals(request.userId())) {
            throw new IllegalArgumentException("Category must belong to the specified user.");
        }

        if (category.getType() != request.type()) {
            throw new IllegalArgumentException(String.format(
                    "Template transaction type (%s) must match Category type (%s).",
                    request.type(), category.getType()
            ));
        }

        RecurringTransaction template = new RecurringTransaction();
        template.setUser(user);
        template.setCategory(category);
        template.setType(request.type());
        template.setName(request.name());
        template.setAmount(request.amount());
        template.setNextDueDate(request.nextDueDate());
        template.setFrequency(request.frequency());
        template.setActive(true);

        RecurringTransaction savedTemplate = recurringTransactionRepository.save(template);
        return mapToResponse(savedTemplate);
    }

    @Transactional
    public RecurringTransactionResponse updateTemplate(Long id, UpdateRecurringTransactionRequest request) {
        RecurringTransaction template = recurringTransactionRepository.findById(id)
                .orElseThrow(() -> new IllegalArgumentException("Recurring template not found with ID: " + id));

        Category category = categoryRepository.findById(request.categoryId())
                .orElseThrow(() -> new IllegalArgumentException("Category not found with ID: " + request.categoryId()));

        if (!category.getUser().getId().equals(template.getUser().getId())) {
            throw new IllegalArgumentException("Category must belong to the template owner.");
        }

        if (category.getType() != template.getType()) {
            throw new IllegalArgumentException(String.format(
                    "Cannot assign to Category '%s' (%s) because this is an %s recurring template.",
                    category.getName(), category.getType(), template.getType()
            ));
        }

        template.setCategory(category);
        template.setName(request.name());
        template.setAmount(request.amount());
        template.setNextDueDate(request.nextDueDate());
        template.setFrequency(request.frequency());
        template.setActive(request.isActive());

        RecurringTransaction updatedTemplate = recurringTransactionRepository.save(template);
        return mapToResponse(updatedTemplate);
    }

    @Transactional
    public void deleteTemplate(Long id) {
        if (!recurringTransactionRepository.existsById(id)) {
            throw new IllegalArgumentException("Recurring template not found with ID: " + id);
        }

        boolean isLinkedToTransactions = transactionRepository.existsBySourceTemplate_Id(id);
        if (isLinkedToTransactions) {
            throw new ResourceConflictException("Cannot delete this template because it has generated past transaction records. Disable it instead.");
        }

        recurringTransactionRepository.deleteById(id);
    }

    private RecurringTransactionResponse mapToResponse(RecurringTransaction template) {
        return new RecurringTransactionResponse(
                template.getId(),
                template.getUser().getId(),
                template.getCategory().getId(),
                template.getCategory().getName(),
                template.getType(),
                template.getName(),
                template.getAmount(),
                template.getNextDueDate(),
                template.getFrequency(),
                template.isActive(),
                template.getCreatedAt(),
                template.getUpdatedAt()
        );
    }
}
