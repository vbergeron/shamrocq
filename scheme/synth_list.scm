(define append (lambdas (l1 l2)
  (match l1
    ((Nil) l2)
    ((Cons x xs) `(Cons ,x ,(@ append xs l2))))))

(define rev_aux (lambdas (acc l)
  (match l
    ((Nil) acc)
    ((Cons x xs) (@ rev_aux `(Cons ,x ,acc) xs)))))

(define reverse (lambda (l) (@ rev_aux `(Nil) l)))

(define nth (lambdas (n l)
  (match l
    ((Nil) `(None_))
    ((Cons x xs) (match n
      ((O) `(Some ,x))
      ((S n~) (@ nth n~ xs)))))))

(define list_map (lambdas (f l)
  (match l
    ((Nil) `(Nil))
    ((Cons x xs) `(Cons ,(f x) ,(@ list_map f xs))))))

(define list_filter (lambdas (f l)
  (match l
    ((Nil) `(Nil))
    ((Cons x xs)
      (match (f x)
        ((True) `(Cons ,x ,(@ list_filter f xs)))
        ((False) (@ list_filter f xs)))))))

(define lrange (lambdas (lo hi)
  (if (< lo hi)
    `(Cons ,lo ,(@ lrange (+ lo 1) hi))
    `(Nil))))

(define is_positive (lambda (x) (< 0 x)))

(define zip (lambdas (l1 l2)
  (match l1
    ((Nil) `(Nil))
    ((Cons x xs) (match l2
      ((Nil) `(Nil))
      ((Cons y ys) `(Cons ,`(Pair ,x ,y) ,(@ zip xs ys))))))))
