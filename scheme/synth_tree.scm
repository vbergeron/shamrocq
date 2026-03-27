(define tree_empty `(Leaf `(Nil)))

(define tree_insert (lambdas (ord x t)
  (match t
    ((Leaf _) `(Node ,`(Leaf ,`(Nil)) ,x ,`(Leaf ,`(Nil))))
    ((Node left val right)
      (match (@ ord x val)
        ((Left) (match (@ ord val x)
          ((Left) t)
          ((Right) `(Node ,(@ tree_insert ord x left) ,val ,right))))
        ((Right) `(Node ,left ,val ,(@ tree_insert ord x right))))))))

(define tree_member (lambdas (ord x t)
  (match t
    ((Leaf _) `(False))
    ((Node left val right)
      (match (@ ord x val)
        ((Left) (match (@ ord val x)
          ((Left) `(True))
          ((Right) (@ tree_member ord x left))))
        ((Right) (@ tree_member ord x right)))))))

(define tree_size (lambda (t)
  (match t
    ((Leaf _) `(O))
    ((Node left _ right) `(S ,(@ add (tree_size left) (tree_size right)))))))

(define tree_height (lambda (t)
  (match t
    ((Leaf _) `(O))
    ((Node left _ right) `(S ,(@ max_nat (tree_height left) (tree_height right)))))))

(define tree_to_list (lambda (t)
  (match t
    ((Leaf _) `(Nil))
    ((Node left val right)
      (@ append (tree_to_list left) `(Cons ,val ,(tree_to_list right)))))))
