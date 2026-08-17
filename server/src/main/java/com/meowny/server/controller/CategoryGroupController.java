package com.meowny.server.controller;

import com.meowny.server.dto.categorygroup.CategoryGroupResponse;
import com.meowny.server.dto.categorygroup.CreateCategoryGroupRequest;
import com.meowny.server.dto.categorygroup.UpdateCategoryGroupRequest;
import com.meowny.server.service.CategoryGroupService;
import jakarta.validation.Valid;
import jakarta.validation.constraints.NotNull;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.validation.annotation.Validated;
import org.springframework.web.bind.annotation.*;

import java.util.List;

@RestController
@RequestMapping("/api/v1/category-groups")
@Validated
public class CategoryGroupController {

    private final CategoryGroupService categoryGroupService;

    public CategoryGroupController(CategoryGroupService categoryGroupService) {
        this.categoryGroupService = categoryGroupService;
    }

    @GetMapping
    public ResponseEntity<List<CategoryGroupResponse>> getCategoryGroupsByUserId(
            @RequestParam @NotNull Long userId) {

        List<CategoryGroupResponse> responses = categoryGroupService.getCategoryGroupsByUserId(userId);
        return ResponseEntity.ok(responses);
    }

    @PostMapping
    public ResponseEntity<CategoryGroupResponse> createCategoryGroup(
            @Valid @RequestBody CreateCategoryGroupRequest request) {

        CategoryGroupResponse response = categoryGroupService.createCategoryGroup(request);
        return ResponseEntity.status(HttpStatus.CREATED).body(response);
    }

    @PutMapping("/{id}")
    public ResponseEntity<CategoryGroupResponse> updateCategoryGroup(
            @PathVariable Long id,
            @Valid @RequestBody UpdateCategoryGroupRequest request) {

        CategoryGroupResponse response = categoryGroupService.updateCategoryGroup(id, request);
        return ResponseEntity.ok(response);
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> deleteCategoryGroup(@PathVariable Long id) {
        categoryGroupService.deleteCategoryGroup(id);
        return ResponseEntity.noContent().build();
    }
}
