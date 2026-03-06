package tree_sitter_plank_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_plank "github.com/plankevm/plank-monorepo/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_plank.Language())
	if language == nil {
		t.Errorf("Error loading Plank grammar")
	}
}
